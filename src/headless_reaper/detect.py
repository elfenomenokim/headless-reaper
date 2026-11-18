"""Decide which browsers are orphaned automation, and group them.

A process is a target only when all of these hold:

1. its executable is a browser family this tool knows (Chrome, Chromium, Edge,
   Chrome for Testing, the Playwright builds, Firefox, WebKit);
2. it is a browser main process, not a helper of one;
3. it carries an automation signal: `--headless`, or a temp/automation
   `--user-data-dir` (a visible browser with a real user profile never carries
   one, and is excluded explicitly below);
4. its launcher is gone: the parent is PID 1, an init/systemd process, or the
   parent PID no longer exists.

Children of a qualifying root are grouped with it, so the report gives the RSS
of a whole browser tree rather than of one process.
"""

from __future__ import annotations

import os
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

from .proc import Proc

__all__ = [
    "Group",
    "find_groups",
    "has_headless",
    "has_remote_debugging",
    "is_automation_profile",
    "is_browser",
    "is_default_profile",
    "is_helper",
    "is_temp_path",
    "profile_path",
    "raw_profile",
]

BROWSER_BASENAMES = frozenset(
    {
        "chrome",
        "chrome.exe",
        "google chrome",
        "google-chrome",
        "google-chrome-stable",
        "google-chrome-beta",
        "google-chrome-unstable",
        "chromium",
        "chromium-browser",
        "chromium-browser-stable",
        "chromium.exe",
        "chrome-headless-shell",
        "chrome-headless-shell.exe",
        "headless_shell",
        "headless-shell",
        "msedge",
        "msedge.exe",
        "microsoft edge",
        "microsoft-edge",
        "microsoft-edge-stable",
        "firefox",
        "firefox-bin",
        "firefox.exe",
        "nightly",
        "pw_run.sh",
        "playwright",
        "minibrowser",
    }
)

BROWSER_NAME_PREFIXES = (
    "chrome",
    "chromium",
    "headless",
    "firefox",
    "nightly",
    "pw_",
    "playwright",
    "minibrowser",
    "webkit",
)

AUTOMATION_PATH_MARKERS = ("ms-playwright", "chrome-for-testing", "puppeteer", "playwright")
HELPER_NAME_MARKERS = ("helper", "crashpad_handler", "webprocess", "networkprocess", "gpuprocess")
INIT_LIKE = frozenset({"launchd", "init", "systemd"})

_CHROME_PROFILE_OPTIONS = ("--user-data-dir", "-user-data-dir")
_FIREFOX_PROFILE_OPTIONS = ("-profile", "--profile")

_AUTOMATION_PROFILE_RE = re.compile(
    r"(?:^playwright_[a-z0-9_]*profile[-_]"
    r"|^puppeteer_dev_[a-z0-9_]*profile"
    r"|^scoped_dir"
    r"|^\.org\.chromium\.chromium\.)",
    re.IGNORECASE,
)


def _temp_roots() -> tuple[str, ...]:
    roots = {"/tmp", "/var/tmp", "/private/tmp", "/private/var/tmp", "/dev/shm"}
    try:
        roots.add(tempfile.gettempdir())
    except (OSError, RuntimeError):
        pass
    return tuple(sorted(os.path.realpath(root) for root in roots))


_TEMP_ROOTS = _temp_roots()


def _default_profile_roots() -> tuple[str, ...]:
    home = Path.home()
    roots: list[Path] = []
    if sys.platform == "darwin":
        support = home / "Library" / "Application Support"
        roots += [
            support / "Google" / "Chrome",
            support / "Google" / "Chrome Beta",
            support / "Google" / "Chrome Dev",
            support / "Chromium",
            support / "Microsoft Edge",
            support / "BraveSoftware" / "Brave-Browser",
            support / "Firefox",
        ]
    elif sys.platform.startswith("linux"):
        config = home / ".config"
        roots += [
            config / "google-chrome",
            config / "google-chrome-beta",
            config / "chromium",
            config / "microsoft-edge",
            config / "BraveSoftware" / "Brave-Browser",
            home / ".mozilla" / "firefox",
        ]
    return tuple(os.path.realpath(str(root)) for root in roots)


_DEFAULT_PROFILE_ROOTS = _default_profile_roots()


def is_browser(proc: Proc) -> bool:
    """True when the executable belongs to a browser family this tool knows."""
    name = proc.name.lower()
    if name in BROWSER_BASENAMES:
        return True
    if any(marker in proc.exe.lower() for marker in AUTOMATION_PATH_MARKERS):
        return name.startswith(BROWSER_NAME_PREFIXES)
    return False


def is_helper(proc: Proc) -> bool:
    """True for a browser helper process rather than a browser main process."""
    if any(marker in proc.name.lower() for marker in HELPER_NAME_MARKERS):
        return True
    for token in proc.argv:
        lowered = token.lower()
        if lowered.startswith("--type=") or lowered in ("-contentproc", "--contentproc"):
            return True
    return False


def has_headless(proc: Proc) -> bool:
    for token in proc.argv:
        lowered = token.lower()
        if lowered in ("--headless", "-headless"):
            return True
        if lowered.startswith("--headless=") or lowered.startswith("-headless="):
            return True
    return False


def has_remote_debugging(proc: Proc) -> bool:
    return any(token.lower().startswith("--remote-debugging-") for token in proc.argv)


def _option_value(proc: Proc, names: tuple[str, ...]) -> str | None:
    for index, token in enumerate(proc.argv):
        if not token.startswith("-"):
            continue
        key, separator, value = token.partition("=")
        if key not in names:
            continue
        if separator:
            return value or None
        following = proc.argv[index + 1] if index + 1 < len(proc.argv) else ""
        if following and not following.startswith("-"):
            return following
        return None
    return None


def raw_profile(proc: Proc) -> str | None:
    """The profile argument exactly as the process carries it (may be relative)."""
    return _option_value(proc, _CHROME_PROFILE_OPTIONS) or _option_value(
        proc, _FIREFOX_PROFILE_OPTIONS
    )


def profile_path(proc: Proc) -> str | None:
    """The Chrome `--user-data-dir` or Firefox `-profile` path, as an absolute path.

    A relative argument belongs to the *browser's* working directory, never to
    this tool's.  When that directory cannot be determined the profile cannot
    be safely located; returning ``None`` keeps the directory from being
    misclassified or deleted.
    """
    value = raw_profile(proc)
    if not value:
        return None
    value = os.path.expanduser(value)
    if not os.path.isabs(value):
        if proc.cwd is None:
            return None
        value = os.path.join(proc.cwd, value)
    return os.path.abspath(value)


def is_temp_path(path: str) -> bool:
    real = os.path.realpath(path)
    for root in _TEMP_ROOTS:
        if real == root or real.startswith(root + os.sep):
            return True
    # macOS per-user temp directories, even when TMPDIR is not set in this shell.
    if "/var/folders/" in real and "/T/" in real:
        return True
    return False


def is_automation_profile(path: str) -> bool:
    """True for a profile that a test or automation tool owns, not a person."""
    if is_temp_path(path):
        return True
    real = os.path.realpath(path)
    if _AUTOMATION_PROFILE_RE.search(os.path.basename(real)):
        return True
    lowered = real.lower()
    return any(
        marker in lowered
        for marker in ("/ms-playwright/", "/chrome-for-testing/", "/puppeteer/")
    )


def is_default_profile(path: str) -> bool:
    """True for a browser's real user profile directory; never a target."""
    real = os.path.realpath(path)
    for root in _DEFAULT_PROFILE_ROOTS:
        if real == root or real.startswith(root + os.sep):
            return True
    return False


def _launcher_gone(proc: Proc, by_pid: dict[int, Proc]) -> tuple[bool, str]:
    if proc.ppid == 1:
        parent = by_pid.get(1)
        where = f" to {parent.name} 1" if parent is not None else " to init 1"
        return True, f"launcher gone (reparented{where})"
    parent = by_pid.get(proc.ppid)
    if parent is None:
        return True, f"launcher gone (parent pid {proc.ppid} exited)"
    if parent.name.lower() in INIT_LIKE:
        return True, f"launcher gone (reparented to {parent.name} {parent.pid})"
    return False, f"parent {parent.name} {parent.pid} is alive"


@dataclass(frozen=True)
class _Candidate:
    proc: Proc
    profile: str | None
    key: str
    signals: tuple[str, ...]


@dataclass(frozen=True)
class Group:
    """One orphaned browser tree, rooted at its main process."""

    root: Proc
    members: tuple[Proc, ...]
    profile: str | None
    signals: tuple[str, ...]
    reason: str

    @property
    def pids(self) -> list[int]:
        return [proc.pid for proc in self.members]

    def rss_bytes(self) -> int:
        return sum(proc.rss_bytes for proc in self.members)

    def age_seconds(self, now: float) -> float:
        return now - min(proc.started_at for proc in self.members)


def _candidate(proc: Proc) -> _Candidate | None:
    if not is_browser(proc) or is_helper(proc):
        return None
    profile = profile_path(proc)
    if profile is not None and is_default_profile(profile):
        return None

    signals: list[str] = []
    if has_headless(proc):
        signals.append("headless")
    automation_profile = profile is not None and is_automation_profile(profile)
    if automation_profile:
        signals.append("temp profile" if is_temp_path(profile) else "automation profile")
    if has_remote_debugging(proc) and automation_profile:
        signals.append("remote debugging")
    # A visible browser started against a real profile is never a target, even
    # when it carries a debugging switch. Without either signal it is skipped.
    if not signals:
        return None
    key = os.path.realpath(profile) if profile is not None else f"pid:{proc.pid}"
    return _Candidate(proc=proc, profile=profile, key=key, signals=tuple(signals))


def _members(
    root: Proc,
    key: str,
    procs: list[Proc],
    children: dict[int, list[Proc]],
    by_pid: dict[int, Proc],
) -> tuple[Proc, ...]:
    collected: dict[int, Proc] = {root.pid: root}
    stack = [root.pid]
    while stack:
        for child in children.get(stack.pop(), ()):
            if child.pid in collected:
                continue
            collected[child.pid] = child
            stack.append(child.pid)

    # Helpers can outlive their root and be reparented to init. They carry the
    # same profile path, so they join the tree instead of becoming loose groups.
    # Only genuinely orphaned helpers: a process whose own launcher is alive
    # must never become a signal target through this back door.
    for proc in procs:
        if proc.pid in collected:
            continue
        if not is_helper(proc):
            continue
        profile = profile_path(proc)
        if profile is None or os.path.realpath(profile) != key:
            continue
        gone, _reason = _launcher_gone(proc, by_pid)
        if not gone:
            continue
        collected[proc.pid] = proc
    return tuple(sorted(collected.values(), key=lambda proc: proc.pid))


def _under(path: str, prefix: str) -> bool:
    real_path = os.path.realpath(path)
    real_prefix = os.path.realpath(prefix)
    return real_path == real_prefix or real_path.startswith(real_prefix + os.sep)


def find_groups(
    procs: list[Proc],
    now: float,
    min_age: float = 0.0,
    paths: tuple[str, ...] = (),
) -> list[Group]:
    """Return orphaned browser groups, oldest first."""
    by_pid = {proc.pid: proc for proc in procs}
    children: dict[int, list[Proc]] = {}
    for proc in procs:
        children.setdefault(proc.ppid, []).append(proc)

    roots: dict[str, tuple[_Candidate, str]] = {}
    for proc in procs:
        candidate = _candidate(proc)
        if candidate is None:
            continue
        gone, reason = _launcher_gone(proc, by_pid)
        if not gone:
            continue
        current = roots.get(candidate.key)
        if current is None or proc.started_at < current[0].proc.started_at:
            roots[candidate.key] = (candidate, reason)

    groups: list[Group] = []
    for key, (candidate, reason) in roots.items():
        group = Group(
            root=candidate.proc,
            members=_members(candidate.proc, key, procs, children, by_pid),
            profile=candidate.profile,
            signals=candidate.signals,
            reason=reason,
        )
        if group.age_seconds(now) < min_age:
            continue
        if paths and (group.profile is None or not any(_under(group.profile, p) for p in paths)):
            continue
        groups.append(group)

    groups.sort(key=lambda group: (-group.age_seconds(now), group.root.pid))
    return groups
