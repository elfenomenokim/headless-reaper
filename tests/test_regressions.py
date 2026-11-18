"""Regression tests for the Codex review fixes. Run from the repo root:

    PYTHONPATH=src python3 -m unittest discover -s tests -v

Process fixtures are synthetic; no real browser is ever signaled.
"""
from __future__ import annotations

import importlib
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from headless_reaper.detect import Group, find_groups, profile_path, raw_profile
from headless_reaper.proc import Proc, _linux_procs, _parse_lsof_cwd

reap = importlib.import_module("headless_reaper.reap")  # the module, not the reap() function

NOW = 1000.0


def proc(pid, ppid, exe, argv, *, cwd=None, started=900.0, state="S"):
    return Proc(pid=pid, ppid=ppid, rss_bytes=0, started_at=started, state=state,
                exe=exe, argv=tuple(argv), cwd=cwd)


def firefox(pid, ppid, profile_arg, *, cwd=None, extra=(), started=900.0):
    return proc(pid, ppid, "/usr/bin/firefox",
                ("firefox", "-profile", profile_arg, *extra), cwd=cwd, started=started)


def chromium(pid, ppid, profile_arg, *, extra=()):
    return proc(pid, ppid, "/usr/bin/chromium",
                ("chromium", f"--user-data-dir={profile_arg}", *extra))


class TestLinuxReaderKeepsInaccessiblePids(unittest.TestCase):
    """A live launcher whose /proc files cannot be read must stay in the snapshot."""

    @unittest.skipIf(os.geteuid() == 0, "chmod 000 is not enforced for root")
    def test_unreadable_entries_are_preserved(self):
        root = Path(tempfile.mkdtemp(prefix="reaper-procfs-"))
        (root / "uptime").write_text("100000.00 200000.00\n")

        def make(pid, ppid, *, cmdline=None, cmdline_mode=None, stat_mode=None):
            d = root / str(pid)
            d.mkdir()
            fields = f"{pid} (proc) S {ppid}" + " 0" * 17 + " 5000 0 100"
            stat = d / "stat"
            stat.write_text(fields)
            if stat_mode is not None:
                os.chmod(stat, stat_mode)
            if cmdline is not None:
                target = d / "cmdline"
                target.write_bytes(cmdline)
                if cmdline_mode is not None:
                    os.chmod(target, cmdline_mode)
            os.symlink("/usr/bin/proc-exe", d / "exe")
            os.symlink("/tmp", d / "cwd")
            return d

        make(100, 1, cmdline=b"firefox\0-profile\0relprof\0")
        make(200, 1, cmdline=b"python3\0launcher.py\0", cmdline_mode=0o000)
        make(300, 1, cmdline=b"x\0", stat_mode=0o000)
        make(400, 1, cmdline=b"")              # kernel thread / exited: dropped

        found = {p.pid: p for p in _linux_procs(NOW, root=str(root))}
        os.chmod(root / "200" / "cmdline", 0o644)
        os.chmod(root / "300" / "stat", 0o644)
        shutil.rmtree(root)
        self.assertEqual(set(found), {100, 200, 300})
        self.assertEqual(found[100].argv, ("firefox", "-profile", "relprof"))
        self.assertEqual(found[100].cwd, "/tmp")
        self.assertEqual(found[200].argv, ())          # unreadable, not missing
        self.assertEqual(found[200].ppid, 1)
        self.assertEqual(found[300].ppid, -1)          # stat unreadable: unknown entry

        from headless_reaper.detect import _launcher_gone
        browser = firefox(500, 200, "/tmp/reaper-fixture/prof")
        gone, _ = _launcher_gone(browser, found)
        self.assertFalse(gone, "an unreadable launcher is a live parent, not a missing one")
        gone, _ = _launcher_gone(firefox(501, 999, "/tmp/reaper-fixture/prof2"), found)
        self.assertTrue(gone)


class TestRelativeProfileResolution(unittest.TestCase):
    def test_relative_profile_uses_browser_cwd(self):
        p = firefox(100, 1, "relprof", cwd="/job")
        self.assertEqual(profile_path(p), "/job/relprof")

    def test_absolute_profile_unaffected(self):
        p = firefox(100, 1, "/tmp/absprof")
        self.assertEqual(profile_path(p), "/tmp/absprof")

    def test_unresolvable_relative_profile_is_none(self):
        p = firefox(100, 1, "relprof", cwd=None)
        self.assertIsNone(profile_path(p))
        self.assertEqual(raw_profile(p), "relprof")

    def test_lsof_cwd_parsing(self):
        parsed = _parse_lsof_cwd("p1\nfcwd\nn/\np123\nn/home/user\np999\n")
        self.assertEqual(parsed, {1: "/", 123: "/home/user"})


class TestSameProfilePassRequiresOrphanedHelper(unittest.TestCase):
    def test_live_launched_browser_does_not_join(self):
        root = chromium(100, 1, "/tmp/reaper-fixture/prof", extra=("--headless",))
        second = chromium(101, 300, "/tmp/reaper-fixture/prof", extra=("--headless",))
        live_helper = chromium(102, 300, "/tmp/reaper-fixture/prof", extra=("--type=renderer",))
        orphan_helper = chromium(103, 1, "/tmp/reaper-fixture/prof", extra=("--type=renderer",))
        launcher = proc(300, 1, "/usr/bin/python3", ("python3", "launcher.py"))
        groups = find_groups([root, second, live_helper, orphan_helper, launcher], NOW)
        self.assertEqual(len(groups), 1)
        self.assertEqual(groups[0].root.pid, 100)
        self.assertEqual(groups[0].pids, [100, 103])


class TestCleanupProfilesChecksSurvivors(unittest.TestCase):
    def test_surviving_member_keeps_profile(self):
        profile = tempfile.mkdtemp(prefix="reaper-cleanup-")
        member = firefox(4242, 1, profile, started=NOW - 5)
        group = Group(root=member, members=(member,), profile=profile,
                      signals=("headless",), reason="test")
        snapshot = [proc(4242, 1, "/usr/bin/firefox",
                         ("firefox", "-profile", profile), started=NOW - 4)]
        with mock.patch.object(reap, "list_processes", return_value=snapshot):
            removed, kept = reap.cleanup_profiles([group])
        self.assertEqual(removed, [])
        self.assertEqual(len(kept), 1)
        self.assertIn("still running", kept[0][1])
        self.assertTrue(os.path.isdir(profile), "profile must not be deleted")

    def test_unresolvable_live_relative_profile_refuses(self):
        profile = tempfile.mkdtemp(prefix="reaper-cleanup-")
        member = firefox(4243, 1, profile, started=NOW - 5)
        group = Group(root=member, members=(member,), profile=profile,
                      signals=("headless",), reason="test")
        live = firefox(7777, 1, "relprof", cwd=None, started=NOW - 4)
        with mock.patch.object(reap, "list_processes", return_value=[live]):
            removed, kept = reap.cleanup_profiles([group])
        self.assertEqual(removed, [])
        self.assertIn("cannot be resolved", kept[0][1])
        self.assertTrue(os.path.isdir(profile))

    def test_dead_group_profile_is_removed(self):
        profile = tempfile.mkdtemp(prefix="reaper-cleanup-")
        member = firefox(4244, 1, profile, started=NOW - 5)
        group = Group(root=member, members=(member,), profile=profile,
                      signals=("headless",), reason="test")
        with mock.patch.object(reap, "list_processes", return_value=[]):
            removed, kept = reap.cleanup_profiles([group])
        self.assertEqual(removed, [profile])
        self.assertEqual(kept, [])
        self.assertFalse(os.path.exists(profile))


if __name__ == "__main__":
    unittest.main()
