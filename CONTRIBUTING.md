# Contributing to headless-reaper

Fixes and reports are welcome. You do not need to ask before opening a pull request for a bug fix.
This tool sends signals and deletes directories, so a safety fix is always welcome.

## Before you start

- **A wrong detection:** open an
  [issue](https://github.com/elfenomenokim/headless-reaper/issues/new/choose) with the JSON output
  (`--json`), your platform and the version. Redact your own paths and browser arguments.
- **A change to what counts as disposable:** open an issue first. A false positive deletes someone's
  browser session.
- **A security problem:** do not open a public issue. See [SECURITY.md](SECURITY.md).

## Set up

```sh
git clone https://github.com/elfenomenokim/headless-reaper
cd headless-reaper
cargo build --release --locked --manifest-path rust/Cargo.toml
cargo test --manifest-path rust/Cargo.toml
PYTHONPATH=src python3 -m unittest discover -s tests
bash rust/parity.sh --json --path /tmp/reaper-parity-empty
```

`docs/demo/orphan.py` starts a disposable headless browser you can reap; it never launches a
personal browser.

## Rules a patch must keep

- Dry run first. The default command reports and changes nothing.
- Never target a browser with a live launcher, and never target a profile outside the temporary
  scope the caller passed with `--path`.
- Keep an inaccessible PID as an unknown live process. An unreadable parent is not a missing
  parent.
- Compare process start times before signalling, so a reused PID cannot become a target.
- Keep both editions aligned; `bash rust/parity.sh` is the check.

## What a good patch has

- **A test or a fixture** that shows the misdetection, and the same command after the fix.
- **A line under Unreleased** in [CHANGELOG.md](CHANGELOG.md) when a user will notice.
- **The platforms you did not test** written in the pull request. The `tests` workflow runs the Rust
  and Python suites plus empty-scope parity on Ubuntu and macOS with Python 3.9 and 3.13; WebKit and
  the unsupported Windows backend remain unexercised.

## Review

Every pull request runs an automatic DeepSeek review. Ask for another pass with `@codex review` or
`@claude review` in a comment. If a finding looks wrong, say why in the pull request.

## Licence

Contributions are licensed under the MIT licence in [LICENSE](LICENSE).
