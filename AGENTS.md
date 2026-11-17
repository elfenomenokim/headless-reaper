# AGENTS.md — headless-reaper

Instructions for an AI agent asked to find or clean up leftover headless browsers.

## When to use it

Use it when a machine is low on memory, a test suite leaves Chrome or Firefox processes running, or the task is "kill the leftover headless browsers". Use the dry run first and read the table; kill only the groups that belong to the job you are working on.

Do not use it to close a browser a person is using. Do not use it as a general process killer: it only matches browser main processes that carry an automation signal and whose launcher is gone.

## Install

```console
$ pipx install headless-reaper
```

`uvx headless-reaper …` runs it without installing. Wheels install the Rust command and keep the pure-Python `headless_reaper` package importable (Python 3.9+). `python -m headless_reaper` explicitly selects the reference. A source install needs Rust; `cargo install --path rust --locked` installs the standalone command. Both editions support macOS/Linux and reject Windows with exit 2.

## Maintaining the editions

- Keep flags, environment variables, table/JSON shapes, qualification and cleanup guards aligned with `src/headless_reaper/`.
- macOS Rust uses libproc for process metadata and sysctl `KERN_PROCARGS2` for exact argv. Retain inaccessible PIDs in the snapshot so an unreadable live parent is never treated as missing. Linux reads `/proc`.
- Run `rust/parity.sh` after building. It compares dynamic ages/RSS by shape, and refuses a mutating comparison with live groups. Test a real throwaway orphan separately; verify a live-parent browser survives.
- `cargo test --manifest-path rust/Cargo.toml` covers safety rules that must not be exercised against a real personal profile.
- Build wheels with maturin `bindings = "bin"`, `module-name = "headless_reaper"`, `python-source = "src"`. Confirm both the command and `import headless_reaper` after installation.
- Update the measured table in both READMEs only from observed runs. The wheel workflow builds artifacts; it has no PyPI publication step.

## Use

```console
$ headless-reaper --json                          # look, machine-readable
$ headless-reaper --kill --json                   # stop the groups it found
$ headless-reaper --path /srv/app/_tmp --kill     # only profiles under one prefix
```

- Dry run is the default. Read `groups[]`; each entry has `root_pid`, `pids`, `processes`, `profile`, `signals`, `reason`, `age`, `rss`.
- Scope with `--path PREFIX` when other jobs are running, and `--min-age 10m` so a browser started a minute ago by a live launcher is left alone.
- `--kill` sends SIGTERM, waits `--grace` (default 10s), then SIGKILL. Exit 0 means every target is gone; exit 1 means one survived; exit 2 means bad usage or an unsupported platform.
- `--remove-profiles` deletes the temp profile of a reaped group. It refuses anything outside a temp root, anything a live browser still uses, and anything holding a singleton lock.

## macOS code-sign clones

- `--clean-clones` alone is dry-run clone inspection, with default `--min-age 10m`; add `--kill` to delete. `--all-leftovers` implies `--kill --clean-clones --remove-profiles`.
- Clone scope is the current user's X temp directory and recognized browser bundle IDs only. No arbitrary root override exists. Directory symlinks are preserved.
- Native executable paths plus lsof open/mapped paths protect live uses. Also compare device/inode: multiple clone names can hard-link a running executable. An open-file scan failure prevents deletion.
- Do not present allocated blocks as physically reclaimable APFS space. Keep the estimate unknown and report the observed free-space change after deletion.
- Run scheduled cleanup after Chrome quits. On other OSes clone-only mode is a no-op; Windows browser reaping remains unsupported.

## Verify before you report

1. The command exited 0 (or 1, and you can name the survivor).
2. A second `headless-reaper` run reports no such group.
3. The processes you expected to keep running still run.

## Do not

- Do not run `--kill` without first reading the dry run when you are unsure which jobs are active.
- Do not add `pkill -f chrome` as a "faster" alternative; it also kills a user's browser windows.
- Do not remove a profile that is not under a temp root, and do not delete a browser profile that holds a person's account data.
- Do not treat a headless browser started directly by launchd or systemd as identical to an orphan; it also reports `launcher gone`, so scope it with `--path` or `--min-age`.
