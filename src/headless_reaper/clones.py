"""Inspect only browser code-signing clones in this user's macOS X temp root."""
from __future__ import annotations

import ctypes
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

from .report import human_bytes

_BUNDLES = frozenset(("com.google.chrome", "org.chromium.chromium", "com.microsoft.edgemac",
                      "com.google.chrome.for_testing", "com.google.chrome.fortesting"))


def _clone_parent(name: str) -> bool:
    suffix = ".code_sign_clone"
    return name.lower().endswith(suffix) and name[:-len(suffix)].lower() in _BUNDLES


def _x_root() -> Path:
    temp = subprocess.check_output(["getconf", "DARWIN_USER_TEMP_DIR"], text=True, timeout=5).strip()
    root = Path(temp).resolve(strict=True).parent / "X"
    if not root.is_dir() or root.is_symlink():
        raise OSError("user X temp directory is unavailable")
    return root


def _open_paths() -> set[str]:
    """Read exact executable paths natively and open/mapped files with lsof.

    SIP denies some descriptor-path queries even for same-user processes.
    lsof can inspect those paths and mapped files through its existing route.
    A failed lsof scan prevents removal of any clone.
    """
    lib = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
    lib.proc_listpids.argtypes = [ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p, ctypes.c_int]
    lib.proc_pidpath.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_uint32]
    size = lib.proc_listpids(1, 0, None, 0)
    if size <= 0:
        raise OSError("cannot enumerate processes for clone cleanup")
    buffer = (ctypes.c_int * (size // 4 + 1024))()
    size = lib.proc_listpids(1, 0, buffer, ctypes.sizeof(buffer))
    if size <= 0:
        raise OSError("cannot enumerate processes for clone cleanup")
    paths: set[str] = set()
    for pid in buffer[:size // 4]:
        if pid <= 0:
            continue
        buf = ctypes.create_string_buffer(4096)
        if lib.proc_pidpath(pid, buf, len(buf)) > 0:
            paths.add(os.fsdecode(buf.value))
    opened = subprocess.run(["lsof", "-nP", "-Fpn"], capture_output=True, text=True, timeout=30)
    if opened.returncode != 0 or opened.stderr.strip():
        raise OSError("open-file scan failed; preserving code-sign clones")
    paths.update(line[1:] for line in opened.stdout.splitlines() if line.startswith("n/"))
    return {os.path.realpath(path) for path in paths}


def _identities(paths: set[str]) -> set[tuple[int, int]]:
    identities = set()
    for path in paths:
        try:
            stat = os.stat(path)
            identities.add((stat.st_dev, stat.st_ino))
        except OSError:
            pass
    return identities


def _size(path: Path, opened: set[tuple[int, int]]) -> tuple[int, int, bool]:
    apparent = allocated = 0
    used = False
    stack = [path]
    while stack:
        entry = stack.pop()
        stat = entry.lstat()
        used |= (stat.st_dev, stat.st_ino) in opened
        apparent += stat.st_size
        allocated += stat.st_blocks * 512
        if entry.is_dir() and not entry.is_symlink():
            stack.extend(entry.iterdir())
    return apparent, allocated, used


def clean_clones(*, kill: bool = False, min_age: float = 600) -> dict:
    result = {"supported": sys.platform == "darwin", "dry_run": not kill, "root": None,
              "min_age_seconds": int(min_age), "candidates": [], "kept": [], "removed": [],
              "apparent_bytes": 0, "allocated_bytes": 0, "reclaimable_bytes_estimate": None,
              "free_space_change_bytes": None, "message": "nothing to do on this OS"}
    if sys.platform != "darwin":
        return result
    result["message"] = ""
    try:
        root = _x_root()
        result["root"] = str(root)
        paths = sorted(p for parent in root.iterdir() if _clone_parent(parent.name)
                       and parent.is_dir() and not parent.is_symlink()
                       for p in parent.iterdir() if p.name.startswith("code_sign_clone."))
        if not paths:
            return result
        opened = _open_paths()
        identities = _identities(opened)
    except (OSError, subprocess.SubprocessError) as error:
        result["message"] = f"clone scan unavailable: {error}"
        return result
    now = time.time()
    for path in paths:
        try:
            stat = path.lstat()
            if not path.is_dir() or path.is_symlink() or path.resolve() != path:
                result["kept"].append({"path": str(path), "reason": "not a plain clone directory"})
                continue
            if now - stat.st_birthtime < min_age:
                result["kept"].append({"path": str(path), "reason": "younger than min age"})
                continue
            if any(p == str(path) or p.startswith(str(path) + os.sep) for p in opened):
                result["kept"].append({"path": str(path), "reason": "used by a running process"})
                continue
            apparent, allocated, used = _size(path, identities)
            if used:
                result["kept"].append({"path": str(path), "reason": "used by a running process"})
                continue
            result["candidates"].append({"path": str(path), "age_seconds": int(max(0, now - stat.st_birthtime)),
                                         "apparent_bytes": apparent, "allocated_bytes": allocated})
            result["apparent_bytes"] += apparent
            result["allocated_bytes"] += allocated
        except OSError:
            result["kept"].append({"path": str(path), "reason": "clone directory is unreadable or changed"})
    if kill and result["candidates"]:
        before = shutil.disk_usage(root).free
        try:
            # Recheck after sizing: a launch may have claimed a clone meanwhile.
            opened = _open_paths()
            identities = _identities(opened)
        except (OSError, subprocess.SubprocessError) as error:
            result["message"] = f"clone scan unavailable: {error}"
            return result
        for candidate in result["candidates"]:
            path = Path(candidate["path"])
            try:
                if path.is_symlink() or path.parent.is_symlink() or path.resolve() != path:
                    result["kept"].append({"path": str(path), "reason": "clone directory changed"})
                elif time.time() - path.stat().st_birthtime < min_age:
                    result["kept"].append({"path": str(path), "reason": "younger than min age"})
                elif any(p == str(path) or p.startswith(str(path) + os.sep) for p in opened) or _size(path, identities)[2]:
                    result["kept"].append({"path": str(path), "reason": "used by a running process"})
                else:
                    try:
                        shutil.rmtree(path)
                        result["removed"].append(str(path))
                    except OSError:
                        result["kept"].append({"path": str(path), "reason": "clone deletion failed"})
            except OSError:
                result["kept"].append({"path": str(path), "reason": "clone directory changed"})
        result["free_space_change_bytes"] = shutil.disk_usage(root).free - before
    return result


def render_clones(result: dict) -> str:
    if not result["supported"]:
        return "code-sign clones: nothing to do on this OS."
    if result["message"]:
        return f"code-sign clones: {result['message']}."
    text = (f"code-sign clones: {len(result['candidates'])} eligible, {len(result['kept'])} kept, "
            f"{human_bytes(result['apparent_bytes'])} apparent; reclaimable space: unknown (shared APFS blocks).")
    if not result["dry_run"]:
        text += f"\nremoved {len(result['removed'])} code-sign clones."
        change = result["free_space_change_bytes"]
        if change is not None:
            text += f" filesystem free-space change: {'-' if change < 0 else '+'}{human_bytes(abs(change))}."
    else:
        text += "\ndry run: no code-sign clones were removed. Rerun with --kill --clean-clones."
    return text
