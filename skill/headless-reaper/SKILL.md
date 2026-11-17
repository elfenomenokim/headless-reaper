---
name: headless-reaper
description: Find and stop orphaned headless browsers (Chrome, Chromium, Edge, Playwright, Puppeteer, Firefox, WebKit) whose launcher exited and which still hold memory. Use when a machine is short on memory, a test run leaves browser processes behind, or the task says "kill the leftover headless browsers". Also inspects unused browser code-sign clones in the macOS user X temp directory. Dry run first. Do not use it to close a browser a person is using, or as a general process killer.
---

# headless-reaper

Kill only the headless browsers that a test or automation tool abandoned.

## When this skill applies

A machine is low on memory or swap, a browser test suite leaves processes behind, or a request says "there are 40 chrome processes, clean them up". The tool reports each browser tree with its age, RSS, profile path and the reason it qualifies.

It does not touch: a visible browser, a browser on a real user profile, a browser whose parent process is alive, or a non-browser process.

## Steps

1. Install once:

   ```console
   $ pipx install headless-reaper
   ```

2. Look before killing, and read the table:

   ```console
   $ headless-reaper --json
   ```

   Each group has `root_pid`, `pids`, `profile`, `signals` and `reason`. If the groups belong to another running job, stop here and report them.

3. Kill the groups you own:

   ```console
   $ headless-reaper --kill --min-age 10m
   ```

   Add `--path /srv/app/_tmp` when only one project's profiles should be in scope. `--grace 5s` shortens the wait between SIGTERM and SIGKILL.

4. Check the result: run `headless-reaper` again and confirm the same groups are gone. Exit code 0 means every target stopped, 1 means a survivor remains (the report names it), 2 means a usage or platform error.

5. Only if the request also says to remove profiles:

   ```console
   $ headless-reaper --kill --remove-profiles
   ```

   The tool deletes a profile only when it is under a temp root, no live browser uses it, and no singleton lock is held.

6. For macOS code-signing leftovers:

   ```console
   $ headless-reaper --clean-clones
   $ headless-reaper --kill --clean-clones --min-age 30m
   ```

   Inspect first. The default clone age is 10 minutes. Run scheduled cleanup after Chrome quits; open files and shared hard-link identities preserve clones a running browser still uses. Clone scope is fixed to this user's X temp directory, independent of `--path`. `--all-leftovers` means `--kill --clean-clones --remove-profiles`. Other OSes report that clone cleanup has nothing to do.

## Notes

- A headless browser started directly by launchd or systemd also reports `launcher gone`. Scope with `--path` or `--min-age` when such a service exists.
- Do not replace this with `pkill -f chrome`; that kills the user's browser windows too.
- macOS and Linux only; Windows is rejected with a clear message.
- Python API: `list_processes()`, `find_groups(procs, now, min_age, paths)`, `reap(groups)`, `cleanup_profiles(groups)`, `clean_clones(kill=False, min_age=600)`.
