"""Allow `python -m headless_reaper`."""

from .cli import main

if __name__ == "__main__":
    raise SystemExit(main())
