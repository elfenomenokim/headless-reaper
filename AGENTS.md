# AGENTS.md

## Set up for a user

Goal: `headless-reaper` runs on the user's machine and a dry run reports no leftover headless browsers. It targets browsers that a test or automation job left behind, never the user's own browser.

1. Check what is present.
   - `uname -s` prints `Darwin` or `Linux` (Windows exits 2 by design; stop and say so).
   - `uv --version` prints a version. If not: `curl -LsSf https://astral.sh/uv/install.sh | sh`, then `export PATH="$HOME/.local/bin:$PATH"` (or open a new shell).
2. Install: `uv tool install headless-reaper`. It ends with `Installed 1 executable: headless-reaper`; `headless-reaper --version` prints `headless-reaper 0.1.5` or newer. (`uv tool update-shell` puts `~/.local/bin` on PATH for later shells.) No Rust is needed; the wheel carries the binary.
3. Nothing here needs the person, except knowing which job's browsers to clean: ask "어떤 작업(테스트, 자동화)이 띄운 브라우저를 정리할까요? 그 작업의 임시 프로필 폴더(`/tmp/...`)를 알려주세요." Do not guess a path.
4. Prove it, read-only: `headless-reaper --path /tmp/reaper-setup-check` prints
   `headless-reaper 0.1.5: no orphaned headless browsers (... processes scanned, paths /tmp/reaper-setup-check).` and exits 0. It signals nothing without `--kill`.
5. Real use: run `headless-reaper --path <that job's profile folder>`, read the groups with the user, then add `--kill` only for groups the user agrees are leftovers.
6. If it fails, in this order:
   - `headless-reaper: command not found`: `~/.local/bin` is not on PATH (step 1).
   - Exit 2 on Windows: unsupported platform, no workaround.
   - It lists a group you did not expect: do not `--kill`; a service started by launchd or systemd can look orphaned. Ask the user whose browser it is.
   - Nothing is listed but a browser is plainly leftover: the profile is not under a temp directory or the browser is not headless; the detector skips those on purpose.

## Layout

- `src/headless_reaper/proc.py`: process snapshots.
- `detect.py`: browser identification, grouping and orphan qualification.
- `reap.py`: signals and temporary profile cleanup.
- `clones.py`: macOS code-signing clone inspection and cleanup.
- `cli.py` / `report.py`: options and table/JSON output.
- `rust/src/`: matching native implementation.
- `rust/parity.sh`: Python/Rust CLI comparison.
- `docs/demo/`: disposable browser launcher, VHS tapes and media.
- `public.json`: exported files and repository metadata.

## Commands

Run from the repository root:

```sh
cargo build --release --locked --manifest-path rust/Cargo.toml
cargo test --manifest-path rust/Cargo.toml
PYTHONPATH=src python3 -m unittest discover -s tests
bash rust/parity.sh --json --path /tmp/reaper-parity-empty
PYTHONPATH=src python3 -m headless_reaper --help
python3 -m compileall -q src
```

Parity normalizes changing ages, RSS and process counts. A mutating comparison
requires an empty scope; its first command would consume a live fixture.
No separate linter is configured. Follow each module's current formatting.

## Safety invariants

- Keep flags, environment variables, JSON fields and exits aligned in both editions.
- Never target a personal browser profile or a browser with a live launcher.
- Retain inaccessible PIDs as unknown live processes; an unreadable parent is
  not a missing parent. Preserve exact argv on Rust macOS and both Linux readers.
- Compare start times before signals so a reused PID cannot become a target.
- Dry-run first; read the groups, then use `--path` to limit cleanup to your job.
- Never use a broad `pkill` against browser names. Signal only the owned fixture.
- Profile removal requires a temporary root beneath a temp directory, never the
  temp root itself, no live use, no active singleton lock and no surviving
  process in the group.
- Clone cleanup stays inside recognized bundle roots under the current user's X
  temp directory. Preserve directory symlinks and hard-linked live executables.
- Failed open/mapped-file inspection prevents clone deletion.
- Apparent or allocated bytes are not physically reclaimable APFS space.
- A launchd/systemd child can look orphaned. Age alone does not establish ownership.
- Windows remains an explicit exit-2 unsupported backend, not an empty success.

## Demo and release

- Use `docs/demo/orphan.py` only with a dedicated headless shell and a literal
  `/tmp/reaper-demo.*` profile. Never launch Google Chrome for a recording.
- After cleanup, inspect one scoped scan and check the fixture has no survivors.
- `docs/demo/record.sh` records one VHS process at a time.
- Keep both READMEs and repository/package descriptions aligned.
- Add media to public.json. Version changes and public uploads need a release instruction.

## Code Review Rules

A change is a real defect when it:

- makes a personal browser profile, or a browser with a live launcher, a cleanup target.
- signals a PID without comparing start times first, so a reused PID can be hit.
- removes a profile while the group has a live process, an active singleton lock, or a root outside the temporary tree, or when the profile is a shared temp root itself.
- reaches outside the recognized bundle roots for clones, or deletes a directory symlink or a hard-linked live executable.
- treats an unreadable parent as a missing parent, or drops an inaccessible PID instead of keeping it as unknown live.
- leaves flags, JSON fields or exit codes apart between Python and Rust, or turns Windows into an empty success instead of exit 2.
- drops one of two orphaned mains that share a profile, with its process tree, from the report.
- introduces a broad `pkill` against browser names.

Not defects: wording, formatting, comment style, requested extra tests, or a renamed helper.
