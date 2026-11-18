# Security policy

## Supported versions

Security fixes go into the latest release. Upgrade before reporting, in case the problem is
already fixed.

| Version | Supported |
| --- | --- |
| 0.1.4 | Yes |
| Older | No |

## Reporting a vulnerability

Report it privately, not in a public issue: use the
[Report a vulnerability](https://github.com/elfenomenokim/headless-reaper/security/advisories/new)
form on the repository's Security tab.

Include the command, the version (`headless-reaper --version`), your platform, and what an attacker
could do with the result. A safety problem that can end a personal browser or delete a real profile
is treated as a security report. You get a reply within a few days.

## In scope

- A signal sent to a process whose launcher is alive, or to a PID that was reused after a snapshot.
- A profile deleted outside the temporary scope, or a profile deleted while a member of its group
  still runs.
- A relative profile path resolved against the wrong working directory, so a personal profile is
  treated as temporary.
- A path, argument or process name from a snapshot that changes what the tool signals, without the
  caller asking for it.

## Not a vulnerability

- Reaping the disposable browser that `docs/demo/orphan.py` starts, or any browser whose launcher
  has exited and whose profile lives in a temporary directory.
- A detection you disagree with while the default dry run is in force. Run `--json`, read the
  groups, and open an issue.
- A browser you started yourself with a temporary profile that the tool then reaps. Use `--path` to
  limit the scope.

## Disclosure

Once a fix is released, the advisory is published with credit to the reporter unless they prefer
otherwise.
