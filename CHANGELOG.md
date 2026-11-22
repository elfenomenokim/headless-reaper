# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses `MAJOR.MINOR.PATCH`.

## [0.1.6] - 2026-11-22

### Changed

- The macOS working-directory snapshot (`_macos_cwd`) reads `lsof` through
  [run-bounded](https://pypi.org/project/run-bounded/) 0.1.8 when the package is
  installed: a wall deadline with no reap after a kill, plus a combined output cap.
  A timed-out, killed or truncated listing is discarded, so an incomplete snapshot
  is never mistaken for a finished one. Without the package the reference edition
  keeps its previous standard-library-only behavior.

## [0.1.5] - 2026-11-18

### Fixed

- `--remove-profiles` refuses a shared temp root itself (`/tmp`, `/var/tmp`, `/dev/shm`, `$TMPDIR`),
  so a profile set to the temp directory can no longer delete the whole tree; both editions require
  the profile to be strictly beneath a temp root.
- Two orphaned browser mains that share one profile are reported as one group with both process
  trees, instead of dropping the later main and its helpers.
- `--min-age` and `--grace` reject a duration above 100 years in both editions; a finite but
  enormous value used to panic the Rust edition after SIGTERM, so SIGKILL was never sent.

## [0.1.4] - 2026-11-18

### Fixed

- A launcher whose process details cannot be read stays in the snapshot as an unknown live
  process, so an inaccessible parent no longer makes its browser eligible for `--kill`.
- A relative profile path is resolved against the browser's working directory.
- The same-profile recovery pass adds orphaned helpers only, never a browser with a live launcher.
- `cleanup_profiles()` checks the fresh snapshot for surviving group members before removing a
  profile.

## [0.1.3] - 2026-11-18

### Fixed

- `--older-than` rejects overflowing and non-ASCII durations in both editions.

## [0.1.2] - 2026-11-18

### Fixed

- `--version` reports the package version from `Cargo.toml` (0.1.1 binaries printed 0.1.0).

## [0.1.1] - 2026-11-18

### Changed

- Documentation release: the README, the Korean README, the demo recordings and the social preview
  were rewritten, and the version moved with them. No behaviour change.

## [0.1.0] - 2026-11-17

### Added

- Python module and Rust binary that find orphaned headless browsers, group them and report them.
- `--kill` and `--remove-profiles`, both off unless asked for.
- `--clean-clones` inspects unused macOS code-signing copies and deletes them with `--kill`;
  `--all-leftovers` combines the three cleanup actions.
- Saved measurements for macOS arm64 and Ubuntu 24.04 arm64.
