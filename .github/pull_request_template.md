## What and why

<!-- What was wrong or missing, and how this change fixes it. Link the issue: Fixes #123 -->

## How to check it

<!-- The commands that show the change working, with their output. -->

## Checklist

- [ ] `cargo test --manifest-path rust/Cargo.toml` passes
- [ ] `PYTHONPATH=src python3 -m unittest discover -s tests` passes
- [ ] `bash rust/parity.sh --json --path /tmp/reaper-parity-empty` passes
- [ ] `CHANGELOG.md` has a line under Unreleased if users will notice the change
- [ ] Anything I could not test is written above (another OS, a browser, a router)
