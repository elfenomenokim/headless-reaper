"""Find and stop orphaned headless browsers left behind by tests and automation."""

from __future__ import annotations

__version__ = "0.1.6"

from .clones import clean_clones  # noqa: E402
from .detect import Group, find_groups  # noqa: E402
from .proc import Proc, UnsupportedPlatform, list_processes  # noqa: E402
from .reap import ReapResult, cleanup_profiles, reap  # noqa: E402

__all__ = [
    "Group",
    "Proc",
    "ReapResult",
    "UnsupportedPlatform",
    "__version__",
    "cleanup_profiles",
    "clean_clones",
    "find_groups",
    "list_processes",
    "reap",
]
