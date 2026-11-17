# headless-reaper

[한국어](https://github.com/elfenomenokim/headless-reaper/blob/main/README.ko.md)

Find the headless browsers that a test or automation tool abandoned, and stop them — dry run by default.

```console
$ headless-reaper
headless-reaper 0.1.0: 1 orphaned group, 8 processes, 705 MB RSS (998 processes scanned)

ROOT   AGE     RSS     PIDS  PROFILE
13251  1m 12s  705 MB  8     /tmp/hr-readme/profile
       why: headless, temp profile, remote debugging; launcher gone (reparented to launchd 1)

dry run: nothing was killed. Rerun with --kill to terminate these groups.
```

A test launches Chrome in headless mode, then crashes, or calls `process.exit()` before its cleanup runs. The browser is reparented to the init process, keeps its tabs and its renderers, and holds memory for days. One such run left 57 browser processes holding about 24 GB.

This tool reads the process table, groups each browser tree under its main process, and kills a tree only when its launcher is gone. It never touches a browser a person is using.

## Why

- **Only orphans.** A group is killed only when its parent is PID 1, an init process, or a PID that no longer exists. A browser whose launcher is still alive is reported as nothing and left alone.
- **Only automation.** A target must carry an automation signal: `--headless`, a temp or automation `--user-data-dir`, or a remote debugging switch together with such a profile. A visible browser on a real user profile is never a target, even when it has `--remote-debugging-port`.
- **Whole trees.** Renderers, GPU and utility helpers are grouped with their main process, so the reported RSS is the memory the tree actually holds, and one command stops all of it.
- **A safe ladder.** `--kill` sends SIGTERM to every process in the group, waits 10 seconds by default, then sends SIGKILL to what survived. A recycled PID is detected by comparing start times before a signal is sent.
- **Two editions.** The Rust CLI reads macOS through libproc/sysctl and Linux through `/proc`. The Python reference uses only its standard library (`ps` on macOS, `/proc` on Linux). Neither needs a browser or network to scan.
- **Optional profile cleanup.** `--remove-profiles` deletes the temp profile of a reaped group only when it is under a temp root, no live browser uses it, and no singleton lock is held.

## Install

```console
$ pipx install headless-reaper   # recommended: isolated, on PATH
$ uvx headless-reaper --help     # or run without installing
$ pip install headless-reaper    # inside a project
```

Python 3.9 or newer for the Python package. macOS and Linux have process backends. Both editions report Windows as unsupported and exit 2.

### Rust command and Python library in one package

The wheel uses maturin `bindings = "bin"`: `pipx install headless-reaper` installs the native `headless-reaper` executable. The same wheel includes `src/headless_reaper`, so `import headless_reaper` and `python -m headless_reaper` still use the pure-Python reference. No Python extension or interpreter is needed by the Rust executable itself.

Build from this source with a current stable Rust toolchain:

```console
$ cargo install --path rust --locked
$ python -m venv .venv
$ .venv/bin/pip install maturin
$ .venv/bin/maturin build --release --out dist
$ .venv/bin/pip install dist/*.whl
$ .venv/bin/python -c 'import headless_reaper; print(headless_reaper.__version__)'
```

A source install through pip also needs Rust. To run only the reference edition from a checkout, use `PYTHONPATH=src python -m headless_reaper`. The Rust crate is edition 2021; its direct dependencies are `libc` (Unix process APIs) and `serde_json` (JSON encoding). `Cargo.lock` pins their resolved versions.

The Rust CLI keeps the reference's flags, environment variables, table, JSON fields and exit codes. `rust/parity.sh` compares both commands, normalizing changing snapshot counters (scanned processes, age and RSS) while preserving their types. Run `rust/parity.sh --json --path /tmp/your-test` to compare a live fixture. Mutating parity requires an empty scope because the first command consumes any target. Use `cargo test --manifest-path rust/Cargo.toml` for the live-parent, default-profile, helper and path-boundary fixtures.

`.github/workflows/wheels.yml` prepares macOS arm64/x86_64, manylinux x86_64/aarch64 and Windows x86_64 wheels plus an sdist. It builds artifacts only; it does not upload to PyPI.

### Measured editions

Measured 2026-11-17, Rust 1.100.0, release builds. Each time is the median of 20 sequential subprocess runs, including process launch. Startup uses `--version`; scan uses `--json --path /tmp/headless-reaper-bench-empty` and still reads the whole process table. Peak RSS is a separate scan measured with `/usr/bin/time` (`-l` on macOS, `-v` on Linux). Filesystem caches were warm; these are two machines and their current process populations, not a general benchmark.

| OS / CPU | Edition | Startup (ms) | Scan (ms) | Scan peak RSS (KiB) | Binary (bytes) |
|---|---|---:|---:|---:|---:|
| macOS 27 / arm64 | Python 3.12.8 | 36.078 | 122.098 | 23,296 | — |
| macOS 27 / arm64 | Rust | 2.856 | 11.315 | 3,136 | 620,000 |
| Ubuntu 24.04 / arm64 | Python 3.12 | 68.953 | 76.615 | 15,144 | — |
| Ubuntu 24.04 / arm64 | Rust | 0.841 | 5.564 | 1,892 | 594,688 |

Rust's lower scan time and RSS justify preferring the native command in scheduled callers, with `python -m headless_reaper` as the fallback when it is missing.

On both OSes, a deliberate orphan using a literal throwaway profile under `/tmp` passed table/JSON parity, was reaped with zero running survivors, and had its temp profile removed. A second browser with a live parent was preserved. The macOS fixture had 10 processes; the Linux Chromium headless-shell fixture had 6. The 18-case CLI parity suite passed on macOS, Linux and Windows; Windows Rust was built with the GNU x86_64 toolchain and its unsupported-backend response was exercised. The macOS binary also scanned successfully on a second arm64 Mac.

## Usage

### Look

```console
$ headless-reaper
```

This is a dry run: nothing is signalled. The header gives the total, then one row per browser tree with its root PID, age, resident memory, process count and profile directory, and a `why` line that names the signals and the reason the launcher counts as gone.

### Stop

```console
$ headless-reaper --kill
```

```console
headless-reaper 0.1.0: 1 orphaned group, 7 processes, 705.1 MB RSS (1003 processes scanned)

ROOT   AGE     RSS       PIDS  PROFILE
13251  1m 53s  705.1 MB  7     /tmp/hr-readme/profile
       why: headless, temp profile, remote debugging; launcher gone (reparented to launchd 1)

reaped 1 group: sent SIGTERM to 7 of 7 processes, SIGKILL to 0.
```

When a process ignores SIGTERM, the same run ends with `SIGKILL to 1`, and a process that survives even that is named in the output. The exit code is 0 when every target is gone and 1 when one survived.

### Scope it

A shared machine runs several jobs at once. Two filters keep the sweep inside one of them:

```console
$ headless-reaper --path /srv/app/_tmp
$ headless-reaper --kill --path /srv/app/_tmp --min-age 30m
```

`--path` matches the profile directory, not the executable. Repeat it for more than one prefix. `--min-age` skips trees younger than the given duration, so a browser started a minute ago by a job that is still starting up is left alone. Durations are `30s`, `10m`, `2h`, `1d`, or a plain number of seconds.

### Remove the temp profiles

```console
$ headless-reaper --kill --remove-profiles
```

```console
reaped 1 group: sent SIGTERM to 8 of 8 processes, SIGKILL to 0.
removed temp profile: /tmp/hr-readme/profile
```

A profile is kept, with the reason printed, when it is not under a temp root, when a live browser still uses it, when a `SingletonLock` points at a live process, or when the group left a survivor. Playwright and Puppeteer create a fresh profile per launch, so on a long-lived test machine these directories add up too.

### Machine-readable

```console
$ headless-reaper --json | python3 -m json.tool
```

```json
{
    "ok": true,
    "version": "0.1.0",
    "platform": "darwin",
    "dry_run": true,
    "scanned": 1013,
    "min_age_seconds": 0,
    "paths": [
        "/tmp/hr-json"
    ],
    "totals": {
        "groups": 1,
        "processes": 13,
        "rss_bytes": 1320402944
    },
    "groups": [
        {
            "root_pid": 22308,
            "pids": [
                22308,
                22316,
                22317,
                22319,
                22322,
                22324,
                22325,
                22326,
                22328,
                22329,
                22542,
                22560,
                22596
            ],
            "processes": 13,
            "profile": "/tmp/hr-json/profile",
            "signals": [
                "headless",
                "temp profile"
            ],
            "reason": "launcher gone (reparented to launchd 1)",
            "age_seconds": 10,
            "age": "10s",
            "rss_bytes": 1320402944,
            "rss": "1.2 GB"
        }
    ],
    "reap": null
}
```

With `--kill`, `reap` becomes an object with `targets`, `sigterm`, `sigkill`, `survivors`, `already_gone`, `denied`, `removed_profiles` and `kept_profiles`. An error prints `{"ok": false, "error": "…"}` and exits non-zero.

### Code-signing clones on macOS

Chrome can leave a code-signing copy of its app bundle under the user's X temp directory on each launch, especially after it is killed. Chromium, Edge and Chrome for Testing use the same kind of directory. On one measured host, 174 clones accounted for 193 GB apparent size; deleting 171 unused clones increased real free space by only 1.5 GB. Hard links and shared APFS blocks explain why directory totals can be much larger than the physical space recovered. The directories still accumulate and distort disk alerts.

```console
$ headless-reaper --clean-clones                   # dry run, default min age 10m
$ headless-reaper --kill --clean-clones --min-age 30m
$ headless-reaper --all-leftovers                  # --kill + --clean-clones + --remove-profiles
```

`--clean-clones` alone inspects clones without scanning browser groups or deleting anything. It examines only `code_sign_clone.*` directories under the recognized browser bundle directories in `$(getconf DARWIN_USER_TEMP_DIR)/../X`. It rejects directory symlinks. `--path` still filters browser profiles; it does not expand or change this fixed clone scope. On Linux and Windows clone-only mode says `nothing to do on this OS`; Windows `--all-leftovers` still exits 2 because the browser backend is unsupported.

A clone must be older than `--min-age` and unused by running processes. Both editions use native `proc_pidpath` for exact executable paths and `lsof -nP` for open and mapped files. Some same-user protected processes refuse native descriptor-path queries, so those queries cannot replace lsof here. Matching includes file device/inode identity: Chrome clones can hard-link the same executable as `/Applications`, and a pathname alone can name a different link on the next scan. A running browser can therefore preserve several shared clones. Run scheduled cleanup after Chrome quits. A failed open-file scan preserves all clones and exits 1.

The dry run reports eligible count and apparent bytes. JSON adds a `clones` object with `candidates`, `kept`, `removed`, `apparent_bytes`, `allocated_bytes`, `reclaimable_bytes_estimate` and `free_space_change_bytes`. Allocated blocks can be shared, so the reclaimable estimate is `null`. A deletion pass reports the observed filesystem free-space change, which can also include concurrent disk activity. It rechecks usage before deleting anything.

The local cleanup fixture removed an aged unused clone and preserved an open file reached through a hard link outside the clone, plus a symlink to a directory outside X. The manual Rust sweep changed the fixture-inclusive count from 6 to 5. It did not quit any real Chrome window.

### From cron or a launcher

The tool is quiet when there is nothing to do, and exits 0 after a successful sweep:

```console
$ 30 4 * * * headless-reaper --kill --min-age 30m
```

`--min-age 30m` is the useful guard for a scheduled sweep: a browser that started five minutes ago belongs to a job that is probably still running, and its launcher is still alive anyway.

## Options

| Option | Default | Meaning |
|---|---|---|
| `--kill` | off | Terminate the groups: SIGTERM, wait, then SIGKILL |
| `--clean-clones` | off | Inspect unused macOS code-sign clones; delete only with `--kill` |
| `--all-leftovers` | off | `--kill --clean-clones --remove-profiles` |
| `--min-age DURATION` | `0` (`10m` with clone cleanup) | Skip groups younger than this (`10m`, `2h`, `1d`) |
| `--path PREFIX` | every path | Only profiles under `PREFIX`; repeatable |
| `--remove-profiles` | off | With `--kill`, delete the temp profile of a reaped group |
| `--grace DURATION` | `10s` | Wait between SIGTERM and SIGKILL |
| `--json` | off | Print one machine-readable object |
| `--version` | | Print the version |

Exit codes: `0` dry run, or every target reaped; `1` a target survived or the process table could not be read; `2` bad usage or an unsupported platform.

## What counts as a target

A process becomes a group root only when all of these hold:

1. Its executable is a browser family the tool knows: Chrome, Chromium, Edge, Chrome for Testing, Playwright's Chromium and headless shell, Puppeteer's Chrome, Firefox (including Playwright's build), or WebKit's mini browser.
2. It is a browser main process, not a helper. A helper carries `--type=…`, `-contentproc`, or a name such as `… Helper`, `WebKitWebProcess`, `chrome_crashpad_handler`.
3. It carries an automation signal: `--headless`, or a `--user-data-dir` that is under a temp root or is shaped like an automation profile (`playwright_*dev_profile-*`, `puppeteer_dev_*_profile-*`, `scoped_dir*`), or a `--remote-debugging-*` switch together with such a profile.
4. Its launcher is gone: the parent is PID 1, a process named `launchd`, `init` or `systemd`, or a PID that no longer exists.

The whole tree under that root is the group. A helper that outlived the root and was reparented to init joins the group through its shared profile path instead of becoming a stray group.

Never a target:

- a browser on macOS's or Linux's real user profile (`~/Library/Application Support/Google/Chrome`, `~/.config/google-chrome`, `~/.mozilla/firefox`, and siblings), even with `--headless`;
- a visible browser without an automation profile, even with `--remote-debugging-port`;
- a browser whose parent process is alive.

## Environment

| Variable | Same as | Example |
|---|---|---|
| `HEADLESS_REAPER_MIN_AGE` | `--min-age` | `30m` |
| `HEADLESS_REAPER_PATH` | `--path` | `/srv/app/_tmp:/srv/other/_tmp` |
| `HEADLESS_REAPER_GRACE` | `--grace` | `5s` |

A command-line flag wins over its variable. `HEADLESS_REAPER_PATH` uses the platform path separator: `:` on macOS and Linux.

## Python API

```python
import time

from headless_reaper import clean_clones, cleanup_profiles, find_groups, list_processes, reap

procs = list_processes()
groups = find_groups(procs, now=time.time(), min_age=600, paths=("/srv/app/_tmp",))
for group in groups:
    print(group.root.pid, group.profile, group.signals, group.rss_bytes())

result = reap(groups)
print(result.sigterm, result.sigkill, result.survivors)

removed, kept = cleanup_profiles(groups)

clones = clean_clones(kill=False, min_age=600)  # macOS only; a dry run
```

`list_processes()` returns `Proc` records (`pid`, `ppid`, `rss_bytes`, `started_at`, `state`, `exe`, `argv`). `find_groups()` returns `Group` records with `root`, `members`, `profile`, `signals`, `reason`, and helpers `pids`, `rss_bytes()`, `age_seconds(now)`.

## Limits

Tested on:

- macOS 27, arm64, Google Chrome 157: a deliberate orphan made by launching headless Chrome from a short-lived parent, then a second one from a live parent. The first was found and stopped, the second was left alone, and the machine's normal Chrome windows were untouched.
- Ubuntu 24.04, arm64, Playwright Chromium headless shell 159 and Playwright Firefox 159: a deliberate orphan for each, found, stopped, and its temp profile removed. The real Playwright launch arguments (`--headless`, `--user-data-dir=/tmp/playwright_chromiumdev_profile-…`, `--remote-debugging-pipe`; `-headless -profile …` for Firefox) are what the detector matches.

Not tested:

- **Windows browser detection.** Both editions deliberately have no backend. The Rust GNU x86_64 build and the matching exit-2 response were checked; no browser is scanned or killed there.
- **WebKit live.** The Playwright WebKit build on the test host was missing its GTK 4 libraries, so only its argument shape (`pw_run.sh --headless --user-data-dir=/tmp/playwright_webkitdev_profile-…`) was checked against the detector, not a running browser.
- **macOS/Linux x86_64.** Their process backends have not been run on x86_64. The Windows x86_64 build only exercises the unsupported-platform branch.
- **Python macOS `ps` and spaces.** The reference cannot recover a profile path containing spaces from `ps`. Rust uses `KERN_PROCARGS2` for exact argv and libproc for PID, parent, start time and RSS. Linux reads exact argv in both editions. An inaccessible macOS PID is retained as an unknown process so its children cannot be mistaken for orphans.
- **A browser started directly by launchd or systemd.** Its parent is the init process, so it looks exactly like an orphan. Scope it with `--path`, or give it a `--min-age` larger than its restart interval.
- **A visible automation browser on a real profile.** Puppeteer with `headless: false` and a project profile under your home directory is not reaped, even if its launcher died. That is deliberate: the tool prefers to leave a visible window running over closing one a person may be using. Put such profiles under a temp root if you want them swept.

## Agents

If you are an AI agent asked to clean up leftover browsers, read [AGENTS.md](https://github.com/elfenomenokim/headless-reaper/blob/main/AGENTS.md). The Claude Code skill lives in [`skill/headless-reaper/SKILL.md`](https://github.com/elfenomenokim/headless-reaper/blob/main/skill/headless-reaper/SKILL.md); install it by copying that folder to `~/.claude/skills/headless-reaper/`.

## License

MIT — see [LICENSE](https://github.com/elfenomenokim/headless-reaper/blob/main/LICENSE).
