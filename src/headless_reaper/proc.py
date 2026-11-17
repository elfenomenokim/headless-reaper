"""Read the process table with the standard library only.

Linux is read from /proc, where argv is exact. macOS has no /proc, so the table
comes from `ps`; `ps` joins argv with spaces, which means a switch value that
itself contains a space (a profile path, for example) cannot be recovered
there. Windows has no backend in this release.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
import time
from dataclasses import dataclass

__all__ = ["Proc", "UnsupportedPlatform", "list_processes"]


class UnsupportedPlatform(RuntimeError):
    """Raised when this operating system has no process backend."""


@dataclass(frozen=True)
class Proc:
    """One process, as seen in a single snapshot."""

    pid: int
    ppid: int
    rss_bytes: int
    started_at: float
    state: str
    exe: str
    argv: tuple[str, ...]

    @property
    def name(self) -> str:
        """Executable file name, without its directory."""
        return os.path.basename(self.exe) or self.exe

    @property
    def args_text(self) -> str:
        return " ".join(self.argv)

    def age(self, now: float) -> float:
        return max(0.0, now - self.started_at)


_PS_ENV = {**os.environ, "LC_ALL": "C"}
_ETIME_RE = re.compile(r"^(?:(\d+)-)?(?:(\d+):)?(\d+):(\d+)$")


def _ps(arguments: list[str]) -> str:
    try:
        completed = subprocess.run(
            ["ps", *arguments],
            capture_output=True,
            text=True,
            timeout=30,
            env=_PS_ENV,
        )
    except (OSError, subprocess.SubprocessError) as error:
        raise RuntimeError(f"cannot read the process table with ps: {error}") from error
    if completed.returncode != 0:
        message = (completed.stderr or completed.stdout or "").strip()
        raise RuntimeError(f"ps failed ({completed.returncode}): {message}")
    return completed.stdout


def _parse_etime(value: str) -> float | None:
    match = _ETIME_RE.match(value)
    if match is None:
        return None
    days, hours, minutes, seconds = (int(part) if part else 0 for part in match.groups())
    return float(days * 86400 + hours * 3600 + minutes * 60 + seconds)


def _macos_procs(now: float) -> list[Proc]:
    table = _ps(["-ww", "-Ao", "pid=,ppid=,rss=,etime=,state=,comm="])
    arguments: dict[int, str] = {}
    for line in _ps(["-ww", "-Ao", "pid=,args="]).splitlines():
        head = line.strip().split(None, 1)
        if len(head) == 2 and head[0].isdigit():
            arguments[int(head[0])] = head[1]

    procs: list[Proc] = []
    for line in table.splitlines():
        fields = line.split(None, 5)
        if len(fields) < 6:
            continue
        try:
            pid = int(fields[0])
            ppid = int(fields[1])
            rss_kb = int(fields[2])
        except ValueError:
            continue
        elapsed = _parse_etime(fields[3])
        if elapsed is None:
            continue
        exe = fields[5]
        procs.append(
            Proc(
                pid=pid,
                ppid=ppid,
                rss_bytes=max(0, rss_kb) * 1024,
                started_at=now - elapsed,
                state=fields[4],
                exe=exe,
                argv=tuple(arguments.get(pid, exe).split()),
            )
        )
    return procs


def _linux_uptime() -> float:
    try:
        with open("/proc/uptime", "rb") as handle:
            return float(handle.read().split()[0])
    except (OSError, ValueError, IndexError):
        return 0.0


def _linux_procs(now: float) -> list[Proc]:
    page_size = os.sysconf("SC_PAGE_SIZE")
    clock_ticks = os.sysconf("SC_CLK_TCK")
    uptime = _linux_uptime()
    procs: list[Proc] = []
    for entry in os.scandir("/proc"):
        if not entry.name.isdigit():
            continue
        pid = int(entry.name)
        try:
            with open(f"/proc/{pid}/stat", "rb") as handle:
                stat = handle.read().decode("utf-8", "replace")
        except OSError:
            continue
        closing = stat.rfind(")")
        if closing < 0:
            continue
        fields = stat[closing + 2 :].split()
        if len(fields) < 22:
            continue
        try:
            state = fields[0]
            ppid = int(fields[1])
            start_ticks = int(fields[19])
            rss_pages = int(fields[21])
        except (IndexError, ValueError):
            continue
        try:
            with open(f"/proc/{pid}/cmdline", "rb") as handle:
                raw = handle.read()
        except OSError:
            raw = b""
        parts = [part.decode("utf-8", "replace") for part in raw.split(b"\0") if part]
        if not parts:
            continue  # kernel thread, or a process that exited mid-scan
        if len(parts) == 1 and " " in parts[0]:
            # Chromium rewrites its process title as one argv entry joined by
            # spaces; the switches are still all there.
            parts = parts[0].split()
        argv = tuple(parts)
        try:
            exe = os.readlink(f"/proc/{pid}/exe")
        except OSError:
            exe = argv[0]
        started_at = now - max(0.0, uptime - start_ticks / clock_ticks)
        procs.append(
            Proc(
                pid=pid,
                ppid=ppid,
                rss_bytes=max(0, rss_pages) * page_size,
                started_at=started_at,
                state=state,
                exe=exe,
                argv=argv,
            )
        )
    return procs


def list_processes(now: float | None = None) -> list[Proc]:
    """Return every visible process as one snapshot."""
    moment = time.time() if now is None else now
    if sys.platform == "darwin":
        return _macos_procs(moment)
    if sys.platform.startswith("linux"):
        return _linux_procs(moment)
    raise UnsupportedPlatform(
        f"no process backend for {sys.platform!r}; this release supports macOS and Linux"
    )
