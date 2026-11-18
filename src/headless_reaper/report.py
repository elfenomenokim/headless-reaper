"""Human-readable output: a group table, a dry-run note, and a reap summary."""

from __future__ import annotations

from .detect import Group
from .reap import ReapResult

__all__ = ["human_bytes", "human_duration", "render_dry_run", "render_reap"]


def human_bytes(count: int) -> str:
    value = float(max(0, count))
    for unit in ("B", "KB", "MB", "GB", "TB"):
        if value < 1024 or unit == "TB":
            if unit == "B":
                return f"{int(value)} B"
            text = f"{value:.1f}"
            return f"{text.removesuffix('.0')} {unit}"
        value /= 1024
    return f"{int(value)} B"


def human_duration(seconds: float) -> str:
    total = int(max(0.0, seconds))
    days, rest = divmod(total, 86400)
    hours, rest = divmod(rest, 3600)
    minutes, secs = divmod(rest, 60)
    if days:
        return f"{days}d {hours}h" if hours else f"{days}d"
    if hours:
        return f"{hours}h {minutes}m" if minutes else f"{hours}h"
    if minutes:
        return f"{minutes}m {secs}s" if secs else f"{minutes}m"
    return f"{secs}s"


def _scope_text(min_age: float, paths: tuple[str, ...]) -> str:
    parts = []
    if min_age > 0:
        parts.append(f"min age {human_duration(min_age)}")
    if paths:
        parts.append("paths " + ", ".join(paths))
    return ", ".join(parts)


def _headline(
    version: str, groups: list[Group], scanned: int, min_age: float, paths: tuple[str, ...]
) -> str:
    scope = _scope_text(min_age, paths)
    suffix = f" ({scanned} processes scanned" + (f", {scope}" if scope else "") + ")"
    if not groups:
        return f"headless-reaper {version}: no orphaned headless browsers{suffix}."
    processes = sum(len(group.members) for group in groups)
    rss = sum(group.rss_bytes() for group in groups)
    noun = "group" if len(groups) == 1 else "groups"
    units = "process" if processes == 1 else "processes"
    return (
        f"headless-reaper {version}: {len(groups)} orphaned {noun}, "
        f"{processes} {units}, {human_bytes(rss)} RSS{suffix}"
    )


def _group_table(groups: list[Group], now: float) -> list[str]:
    headers = ("ROOT", "AGE", "RSS", "PIDS", "PROFILE")
    rows = [
        (
            str(group.root.pid),
            human_duration(group.age_seconds(now)),
            human_bytes(group.rss_bytes()),
            str(len(group.members)),
            group.profile or "(no profile switch)",
        )
        for group in groups
    ]
    widths = [max(len(headers[index]), *(len(row[index]) for row in rows)) for index in range(5)]
    lines = ["  ".join(headers[index].ljust(widths[index]) for index in range(5)).rstrip()]
    for group, row in zip(groups, rows):
        lines.append("  ".join(row[index].ljust(widths[index]) for index in range(5)).rstrip())
        why = ", ".join(group.signals) if group.signals else "automation"
        lines.append(" " * (widths[0] + 2) + f"why: {why}; {group.reason}")
    return lines


def render_dry_run(
    version: str,
    groups: list[Group],
    scanned: int,
    now: float,
    min_age: float,
    paths: tuple[str, ...],
) -> str:
    lines = [_headline(version, groups, scanned, min_age, paths)]
    if groups:
        lines.append("")
        lines.extend(_group_table(groups, now))
        lines.append("")
        lines.append("dry run: nothing was killed. Rerun with --kill to terminate these groups.")
    return "\n".join(lines)


def render_reap(
    version: str,
    groups: list[Group],
    scanned: int,
    now: float,
    min_age: float,
    paths: tuple[str, ...],
    result: ReapResult,
    removed: list[str],
    kept: list[tuple[str, str]],
) -> str:
    lines = [_headline(version, groups, scanned, min_age, paths)]
    if not groups:
        return "\n".join(lines)
    lines.append("")
    lines.extend(_group_table(groups, now))
    lines.append("")
    noun = "group" if len(groups) == 1 else "groups"
    lines.append(
        f"reaped {len(groups)} {noun}: sent SIGTERM to {len(result.sigterm)} of "
        f"{result.targets} processes, SIGKILL to {len(result.sigkill)}."
    )
    if result.already_gone:
        lines.append(f"already gone before the signal: {len(result.already_gone)} processes.")
    if result.denied:
        lines.append(
            "not permitted: "
            + ", ".join(str(pid) for pid in result.denied)
            + " (run as the owner of that process)"
        )
    if result.survivors:
        lines.append(
            "still running after SIGKILL: "
            + ", ".join(str(pid) for pid in result.survivors)
            + " (check permissions, then kill them yourself)"
        )
    for path in removed:
        lines.append(f"removed temp profile: {path}")
    for path, reason in kept:
        lines.append(f"kept profile: {path} ({reason})")
    return "\n".join(lines)
