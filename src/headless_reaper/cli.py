"""Command line entry point: `headless-reaper`."""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import sys
import time

from . import __version__
from .clones import clean_clones, render_clones
from .detect import Group, find_groups
from .proc import UnsupportedPlatform, list_processes
from .reap import ReapResult, cleanup_profiles, reap
from .report import human_bytes, human_duration, render_dry_run, render_reap

ENV_MIN_AGE = "HEADLESS_REAPER_MIN_AGE"
ENV_PATH = "HEADLESS_REAPER_PATH"
ENV_GRACE = "HEADLESS_REAPER_GRACE"

# ASCII digits only: the Rust edition parses the same grammar with
# `char::is_ascii_digit`, so a Unicode decimal digit must fail in both.
_DURATION_RE = re.compile(r"^([0-9]+(?:\.[0-9]+)?)\s*([smhd]?)$", re.IGNORECASE)
_UNIT_SECONDS = {"": 1.0, "s": 1.0, "m": 60.0, "h": 3600.0, "d": 86400.0}
# 100 years. A longer wait is a typo, and the Rust edition cannot represent
# every large f64 in a Duration (from_secs_f64 panics after SIGTERM).
MAX_DURATION_SECONDS = 100.0 * 365.0 * 24.0 * 3600.0


class _Parser(argparse.ArgumentParser):
    """argparse that reports its errors through the one exit path below."""

    def error(self, message: str) -> None:  # type: ignore[override]
        raise ValueError(message)


def parse_duration(text: str, option: str) -> float:
    match = _DURATION_RE.match((text or "").strip())
    if match is not None:
        seconds = float(match.group(1)) * _UNIT_SECONDS[match.group(2).lower()]
        # A digit string long enough to overflow f64 must be a usage error, not
        # an infinity that later crashes the report (`int(inf)`).
        if math.isfinite(seconds) and seconds <= MAX_DURATION_SECONDS:
            return seconds
        if math.isfinite(seconds):
            raise ValueError(
                f"{option}: duration is too large (maximum 100y), got {text!r}"
            )
    raise ValueError(
        f"{option}: expected a duration such as 30s, 10m, 2h or 1d, got {text!r}"
    )


def build_parser() -> argparse.ArgumentParser:
    parser = _Parser(
        prog="headless-reaper",
        description=(
            "Find headless browser processes that a test or automation tool left "
            "behind after its launcher exited. Dry run by default."
        ),
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "A process is a target only when it is a browser main process, carries an\n"
            "automation signal (--headless, a temp/automation --user-data-dir, or a\n"
            "remote debugging switch with such a profile), and its launcher is gone.\n"
            "A visible browser on a real user profile is never touched.\n"
            "\n"
            "exit codes: 0 dry run or every target reaped, 1 a target survived or the\n"
            "process table could not be read, 2 bad usage or an unsupported platform."
        ),
    )
    parser.add_argument(
        "--kill",
        action="store_true",
        help="terminate the groups (SIGTERM, wait, then SIGKILL)",
    )
    parser.add_argument("--clean-clones", action="store_true",
                        help="inspect macOS browser code-sign clones; with --kill, remove unused old clones")
    parser.add_argument("--all-leftovers", action="store_true",
                        help="same as --kill --clean-clones --remove-profiles")
    parser.add_argument(
        "--min-age",
        metavar="DURATION",
        default=None,
        help=f"skip groups younger than this, e.g. 10m or 2h (default: 0, or 10m with --clean-clones; or {ENV_MIN_AGE})",
    )
    parser.add_argument(
        "--path",
        action="append",
        default=[],
        metavar="PREFIX",
        help=(
            "only user-data-dirs under PREFIX; repeatable "
            f"(default: every path; or {ENV_PATH})"
        ),
    )
    parser.add_argument(
        "--remove-profiles",
        action="store_true",
        help="with --kill, delete the temp profile of a reaped group when nothing uses it",
    )
    parser.add_argument(
        "--grace",
        metavar="DURATION",
        default=None,
        help=f"wait between SIGTERM and SIGKILL (default: 10s; or {ENV_GRACE})",
    )
    parser.add_argument(
        "--json", action="store_true", help="print one machine-readable object"
    )
    parser.add_argument("--version", action="version", version=f"headless-reaper {__version__}")
    return parser


def _env_paths() -> tuple[str, ...]:
    raw = os.environ.get(ENV_PATH, "")
    return tuple(part.strip() for part in raw.split(os.pathsep) if part.strip())


def _json_requested(argv: list[str]) -> bool:
    return "--json" in argv


def _fail(message: str, code: int, json_mode: bool) -> int:
    if json_mode:
        print(json.dumps({"ok": False, "error": message}))
    else:
        print(f"headless-reaper: {message}", file=sys.stderr)
    return code


def _group_object(group: Group, now: float) -> dict:
    age = group.age_seconds(now)
    return {
        "root_pid": group.root.pid,
        "pids": group.pids,
        "processes": len(group.members),
        "profile": group.profile,
        "signals": list(group.signals),
        "reason": group.reason,
        "age_seconds": int(age),
        "age": human_duration(age),
        "rss_bytes": group.rss_bytes(),
        "rss": human_bytes(group.rss_bytes()),
    }


def _payload(
    groups: list[Group],
    scanned: int,
    now: float,
    min_age: float,
    paths: tuple[str, ...],
    dry_run: bool,
    result: ReapResult | None,
    removed: list[str],
    kept: list[tuple[str, str]],
    clones: dict | None = None,
) -> dict:
    payload = {
        "ok": True,
        "version": __version__,
        "platform": sys.platform,
        "dry_run": dry_run,
        "scanned": scanned,
        "min_age_seconds": int(min_age),
        "paths": list(paths),
        "totals": {
            "groups": len(groups),
            "processes": sum(len(group.members) for group in groups),
            "rss_bytes": sum(group.rss_bytes() for group in groups),
        },
        "groups": [_group_object(group, now) for group in groups],
        "reap": None,
    }
    if result is not None:
        payload["reap"] = {
            "targets": result.targets,
            "sigterm": result.sigterm,
            "sigkill": result.sigkill,
            "survivors": result.survivors,
            "already_gone": result.already_gone,
            "denied": result.denied,
            "removed_profiles": removed,
            "kept_profiles": [{"path": path, "reason": reason} for path, reason in kept],
        }
    if clones is not None:
        payload["clones"] = clones
        payload["ok"] = not _clone_failed(clones)
    return payload


def _clone_failed(clones: dict | None) -> bool:
    return bool(clones and clones["supported"] and (clones["message"] or any(
        item["reason"] == "clone deletion failed" for item in clones["kept"])))


def _run(
    args: argparse.Namespace, min_age: float, grace: float, paths: tuple[str, ...]
) -> int:
    try:
        procs = [] if args.clean_clones and not args.kill else list_processes()
    except UnsupportedPlatform as error:
        message = str(error) + ("; code-sign clones: nothing to do on this OS" if args.clean_clones else "")
        return _fail(message, 2, args.json)
    except RuntimeError as error:
        return _fail(str(error), 1, args.json)

    now = time.time()
    groups = find_groups(procs, now, min_age=min_age, paths=paths)

    if not args.kill:
        clones = clean_clones(min_age=min_age) if args.clean_clones else None
        if args.json:
            print(json.dumps(_payload(groups, len(procs), now, min_age, paths, True, None, [], [], clones)))
        else:
            print(render_clones(clones) if clones is not None else render_dry_run(__version__, groups, len(procs), now, min_age, paths))
        return 1 if _clone_failed(clones) else 0

    result = reap(groups, grace=grace)
    still_running = set(result.survivors) | set(result.denied)
    clean = [group for group in groups if not still_running.intersection(group.pids)]
    removed, kept = (
        cleanup_profiles(clean) if args.remove_profiles else ([], [])
    )

    clones = clean_clones(kill=True, min_age=min_age) if args.clean_clones else None
    if args.json:
        print(
            json.dumps(
                _payload(
                    groups, len(procs), now, min_age, paths, False, result, removed, kept, clones
                )
            )
        )
    else:
        print(
            render_reap(
                __version__, groups, len(procs), now, min_age, paths, result, removed, kept
            )
        )
        if clones is not None:
            print(render_clones(clones))
    return 1 if result.survivors or _clone_failed(clones) else 0


def main(argv: list[str] | None = None) -> int:
    arguments = list(sys.argv[1:] if argv is None else argv)
    parser = build_parser()
    try:
        args = parser.parse_args(arguments)
        if args.all_leftovers:
            args.kill = args.clean_clones = args.remove_profiles = True
        min_age = parse_duration(
            args.min_age if args.min_age is not None else os.environ.get(ENV_MIN_AGE, "10m" if args.clean_clones else "0"),
            "--min-age",
        )
        grace = parse_duration(
            args.grace if args.grace is not None else os.environ.get(ENV_GRACE, "10s"),
            "--grace",
        )
        paths = tuple(args.path) if args.path else _env_paths()
        if args.remove_profiles and not args.kill:
            raise ValueError("--remove-profiles requires --kill")
        return _run(args, min_age, grace, paths)
    except ValueError as error:
        return _fail(str(error), 2, _json_requested(arguments))


if __name__ == "__main__":
    raise SystemExit(main())
