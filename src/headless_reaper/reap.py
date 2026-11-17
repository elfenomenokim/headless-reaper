"""Terminate orphaned browser groups and optionally remove their temp profiles."""

from __future__ import annotations

import os
import shutil
import signal
import time
from dataclasses import dataclass, field

from .detect import Group, is_browser, is_default_profile, is_temp_path, profile_path
from .proc import Proc, list_processes

__all__ = ["ReapResult", "cleanup_profiles", "reap"]


@dataclass
class ReapResult:
    """What the termination pass did, by process id."""

    targets: int = 0
    sigterm: list[int] = field(default_factory=list)
    sigkill: list[int] = field(default_factory=list)
    survivors: list[int] = field(default_factory=list)
    already_gone: list[int] = field(default_factory=list)
    denied: list[int] = field(default_factory=list)


def _targets(groups: list[Group]) -> dict[int, Proc]:
    targets: dict[int, Proc] = {}
    for group in groups:
        for proc in group.members:
            if proc.pid > 1 and proc.pid != os.getpid():
                targets[proc.pid] = proc
    return targets


def _alive_pids(targets: dict[int, Proc], procs: list[Proc]) -> list[int]:
    """PIDs still running as the same process (not a recycled PID, not a zombie)."""
    current = {proc.pid: proc for proc in procs}
    alive: list[int] = []
    for pid, original in targets.items():
        proc = current.get(pid)
        if proc is None:
            continue
        if abs(proc.started_at - original.started_at) > 2:
            continue
        if proc.state.upper().startswith("Z"):
            continue
        alive.append(pid)
    return alive


def _pid_alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def _wait_gone(pids: list[int], seconds: float) -> None:
    deadline = time.monotonic() + max(0.0, seconds)
    while pids and time.monotonic() < deadline:
        pids = [pid for pid in pids if _pid_alive(pid)]
        if pids:
            time.sleep(0.2)


def reap(groups: list[Group], grace: float = 10.0) -> ReapResult:
    """SIGTERM every process in the groups, then SIGKILL what survives."""
    targets = _targets(groups)
    result = ReapResult(targets=len(targets))
    if not targets:
        return result

    alive_before = set(_alive_pids(targets, list_processes()))
    result.already_gone = sorted(pid for pid in targets if pid not in alive_before)

    for pid in sorted(alive_before):
        try:
            os.kill(pid, signal.SIGTERM)
            result.sigterm.append(pid)
        except ProcessLookupError:
            result.already_gone.append(pid)
        except PermissionError:
            result.denied.append(pid)

    _wait_gone(list(result.sigterm), grace)
    survivors = _alive_pids(targets, list_processes())

    for pid in survivors:
        try:
            os.kill(pid, signal.SIGKILL)
            result.sigkill.append(pid)
        except ProcessLookupError:
            pass
        except PermissionError:
            if pid not in result.denied:
                result.denied.append(pid)

    if survivors:
        _wait_gone(list(survivors), 3.0)
        result.survivors = _alive_pids(targets, list_processes())
    return result


def _profile_refusal(
    path: str, procs: list[Proc], reaped: set[int]
) -> str | None:
    if not os.path.isdir(path):
        return "directory is gone"
    if os.path.islink(path):
        return "path is a symlink"
    if not is_temp_path(path):
        return "not under a temp root"
    if is_default_profile(path):
        return "default user profile"
    real = os.path.realpath(path)
    for proc in procs:
        if proc.pid in reaped or proc.state.upper().startswith("Z"):
            continue
        if not is_browser(proc):
            continue
        profile = profile_path(proc)
        if profile is not None and os.path.realpath(profile) == real:
            return f"a live browser still uses it (pid {proc.pid})"
        if path in proc.args_text or real in proc.args_text:
            return f"a live browser still uses it (pid {proc.pid})"
    lock = os.path.join(path, "SingletonLock")
    if os.path.islink(lock):
        try:
            pid = int(os.readlink(lock).rsplit("-", 1)[1])
        except (OSError, ValueError, IndexError):
            pid = 0
        if pid > 1 and _pid_alive(pid):
            return f"profile lock held by live process {pid}"
    return None


def cleanup_profiles(groups: list[Group]) -> tuple[list[str], list[tuple[str, str]]]:
    """Delete the temp profile of a reaped group, and only when it is safe.

    Returns (removed paths, [(kept path, reason)]).
    """
    removed: list[str] = []
    kept: list[tuple[str, str]] = []
    reaped = {proc.pid for group in groups for proc in group.members}
    procs = list_processes()
    for group in groups:
        path = group.profile
        if path is None:
            continue
        refusal = _profile_refusal(path, procs, reaped)
        if refusal is not None:
            kept.append((path, refusal))
            continue
        try:
            shutil.rmtree(path)
        except OSError as error:
            kept.append((path, f"delete failed: {error.strerror or error}"))
            continue
        removed.append(path)
    return removed, kept
