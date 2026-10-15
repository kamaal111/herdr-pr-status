#!/usr/bin/env python3
"""Exercise the release binary with real Git and fake Herdr/GitHub boundaries."""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import unittest

BINARY = Path(__file__).resolve().parents[1] / "target/release/herdr-pr-status"

FAKE = r'''#!/usr/bin/env python3
import json, os, pathlib, subprocess, sys, time
root = pathlib.Path(os.environ["PR_TEST_ROOT"])
args = sys.argv[1:]
kind = pathlib.Path(sys.argv[0]).name
with (root / "calls").open("a") as f:
    f.write(json.dumps([kind, args]) + "\n")
if kind == "herdr":
    if args == ["api", "snapshot"]:
        print((root / "snapshot.json").read_text())
    elif args == ["plugin", "list", "--json"]:
        print((root / "plugins.json").read_text())
    elif args[:2] == ["workspace", "report-metadata"]:
        with (root / "reports").open("a") as f:
            f.write(json.dumps(args) + "\n")
        if (root / "report-fail").exists():
            print("metadata rejected", file=sys.stderr)
            sys.exit(1)
    else:
        sys.exit(11)
else:
    if (root / "fail").exists():
        print("GitHub unavailable", file=sys.stderr)
        sys.exit(1)
    if args[:2] == ["repo", "view"]:
        print('{"nameWithOwner":"alice/project","url":"https://github.com/alice/project"}')
    elif args[0] == "api":
        if (root / "branch-switch").exists():
            subprocess.run(["git", "checkout", "-q", "other"], check=True)
        if (root / "blocked").exists():
            (root / "entered").touch()
            while not (root / "release").exists():
                time.sleep(.01)
        print((root / "prs.json").read_text())
    else:
        sys.exit(12)
'''


def eventually(assertion, timeout=8):
    deadline = time.monotonic() + timeout
    while True:
        try:
            assertion()
            return
        except AssertionError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(.02)


def pr(number, date="2026-10-01T00:00:00Z", branch="feature", repo="alice/project", state="open"):
    return {"number": number, "created_at": date, "state": state,
            "head": {"ref": branch, "repo": {"full_name": repo}}}


class PluginSmoke(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name in ("herdr", "gh"):
            target = self.bin / name
            target.write_text(FAKE)
            target.chmod(0o755)
        self.env = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ["PATH"],
                        HERDR_BIN_PATH=str(self.bin / "herdr"), PR_TEST_ROOT=str(self.root),
                        HERDR_SOCKET_PATH=str(self.root / "socket"),
                        HERDR_PLUGIN_STATE_DIR=str(self.root / "state"),
                        GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1",
                        GH_PROMPT_DISABLED="1")
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.bind(self.env["HERDR_SOCKET_PATH"])
        self.addCleanup(self.sock.close)
        self.repo = self.root / "project"
        self.git("init", "-q", "-b", "feature", str(self.repo), cwd=self.root)
        self.git("-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-q", "--allow-empty", "-m", "initial")
        self.git("remote", "add", "origin", "git@github.com:alice/project.git")
        self.git("branch", "other")
        self.write("prs.json", [pr(123)])
        self.write("plugins.json", {"result": {"plugins": [{"plugin_id": "herdr-pr-status", "enabled": True}]}})
        self.set_workspaces([self.workspace("w1", self.repo)])
        self.addCleanup(self.stop_watcher)

    def git(self, *args, cwd=None):
        return subprocess.run(["git", *args], cwd=cwd or self.repo, env=self.env,
                              check=True, capture_output=True, text=True)

    def write(self, name, value):
        (self.root / name).write_text(json.dumps(value))

    def workspace(self, identifier, path):
        return {"workspace_id": identifier, "active_tab_id": "t1", "tokens": {"unrelated": "keep"},
                "worktree": {"checkout_path": str(path)}}

    def set_workspaces(self, workspaces, panes=None, layouts=None):
        self.write("snapshot.json", {"result": {"type": "session_snapshot", "snapshot": {
            "workspaces": workspaces, "panes": panes or [], "layouts": layouts or []}}})

    def run_plugin(self, action, check=True):
        return subprocess.run([str(BINARY), action], env=self.env, capture_output=True,
                              text=True, check=check, timeout=12)

    def reports(self):
        return [json.loads(line) for line in (self.root / "reports").read_text().splitlines()]

    def calls(self):
        return [json.loads(line) for line in (self.root / "calls").read_text().splitlines()]

    def watcher_exited(self):
        self.assertIn("PR watcher stopped", next((self.root / "state").glob("*/watch.log")).read_text())

    def stop_watcher(self):
        (self.root / "release").touch()
        for directory in (self.root / "state").glob("session-*"):
            (directory / "stop").touch()
        logs = list((self.root / "state").glob("*/watch.log"))
        if logs:
            eventually(self.watcher_exited, timeout=10)

    def test_newest_pr_in_any_state_and_scoped_metadata(self):
        self.write("prs.json", [pr(123), pr(124, "2026-10-02T00:00:00Z", state="merged"),
                                pr(125, "2026-10-03T00:00:00Z", state="closed")])
        self.run_plugin("refresh")
        self.assertEqual(self.reports(), [["workspace", "report-metadata", "w1", "--source",
            "herdr-pr-status", "--token", "pr_number=#125", "--ttl-ms", "180000"]])
        api_args = [args for kind, args in self.calls() if kind == "gh" and args[0] == "api"][0]
        self.assertIn("state=all", api_args)
        self.assertIn("head=alice:feature", api_args)
        self.assertIn("--paginate", api_args)

    def test_metadata_command_failure_is_reported(self):
        (self.root / "report-fail").touch()
        result = self.run_plugin("refresh", check=False)
        self.assertEqual(result.returncode, 1)
        self.assertIn("metadata rejected", result.stderr)

    def test_empty_result_clears_only_owned_token(self):
        self.write("prs.json", [])
        self.run_plugin("refresh")
        self.assertEqual(self.reports()[0][-2:], ["--clear-token", "pr_number"])

    def test_plain_directory_skips_github_and_clears(self):
        self.set_workspaces([self.workspace("w1", self.root)])
        self.run_plugin("refresh")
        self.assertEqual(self.reports()[0][-2:], ["--clear-token", "pr_number"])
        self.assertEqual([kind for kind, _ in self.calls() if kind == "gh"], [])

    def test_detached_head_skips_github(self):
        self.git("checkout", "-q", "--detach")
        self.run_plugin("refresh")
        self.assertEqual(self.reports()[0][-2:], ["--clear-token", "pr_number"])
        self.assertEqual([kind for kind, _ in self.calls() if kind == "gh"], [])

    def test_linked_worktree_has_independent_branch(self):
        linked = self.root / "linked"
        self.git("worktree", "add", "-q", str(linked), "other")
        self.set_workspaces([self.workspace("w1", self.repo), self.workspace("w2", linked)])
        self.write("prs.json", [pr(123), pr(456, branch="other")])
        self.run_plugin("refresh")
        self.assertEqual([r[6] for r in self.reports()], ["pr_number=#123", "pr_number=#456"])

    def test_duplicate_checkout_is_looked_up_once(self):
        self.set_workspaces([self.workspace("w1", self.repo), self.workspace("w2", self.repo)])
        self.run_plugin("refresh")
        self.assertEqual(len([args for kind, args in self.calls() if kind == "gh" and args[0] == "api"]), 1)
        self.assertEqual(len(self.reports()), 2)

    def test_branch_change_during_lookup_clears_stale_number(self):
        (self.root / "branch-switch").touch()
        self.run_plugin("refresh")
        self.assertEqual(self.reports()[0][-2:], ["--clear-token", "pr_number"])

    def test_lookup_failure_preserves_previous_token_then_recovers(self):
        (self.root / "fail").touch()
        result = self.run_plugin("refresh", check=False)
        self.assertEqual(result.returncode, 1)
        self.assertIn("GitHub unavailable", result.stderr)
        self.assertFalse((self.root / "reports").exists())
        (self.root / "fail").unlink()
        self.run_plugin("refresh")
        self.assertEqual(self.reports()[0][6], "pr_number=#123")

    def test_malformed_snapshot_does_not_report(self):
        (self.root / "snapshot.json").write_text("not-json")
        result = self.run_plugin("refresh", check=False)
        self.assertEqual(result.returncode, 1)
        self.assertFalse((self.root / "reports").exists())

    def test_check_does_not_write_state_or_metadata(self):
        result = self.run_plugin("check")
        self.assertIn("w1: #123", result.stdout)
        self.assertFalse((self.root / "reports").exists())
        self.assertFalse((self.root / "state").exists())

    def test_active_pane_directory_fallback(self):
        self.set_workspaces([{"workspace_id": "w1", "active_tab_id": "t1"}],
            [{"pane_id": "p1", "workspace_id": "w1", "tab_id": "t1", "cwd": str(self.repo)}],
            [{"workspace_id": "w1", "tab_id": "t1", "focused_pane_id": "p1"}])
        self.run_plugin("refresh")
        self.assertEqual(self.reports()[0][6], "pr_number=#123")

    def test_duplicate_workspace_ids_are_rejected(self):
        self.set_workspaces([self.workspace("w1", self.repo), self.workspace("w1", self.repo)])
        result = self.run_plugin("refresh", check=False)
        self.assertEqual(result.returncode, 1)
        self.assertIn("duplicate workspace ID", result.stderr)

    def test_refresh_does_not_overlap(self):
        (self.root / "blocked").touch()
        child = subprocess.Popen([str(BINARY), "refresh"], env=self.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.addCleanup(child.wait)
        self.addCleanup(lambda: (self.root / "release").touch())
        eventually(lambda: self.assertTrue((self.root / "entered").exists()))
        result = self.run_plugin("refresh")
        self.assertIn("already in progress", result.stdout)
        (self.root / "release").touch()
        child.communicate(timeout=8)
        self.assertEqual(child.returncode, 0)
        self.assertEqual(len(self.reports()), 1)

    def test_watcher_is_singleton_and_explicitly_stops(self):
        self.run_plugin("start")
        eventually(lambda: self.assertTrue((self.root / "reports").exists()))
        result = self.run_plugin("start")
        self.assertIn("already running", result.stdout)
        self.run_plugin("stop")
        eventually(self.watcher_exited)
        self.assertEqual(len(self.reports()), 1)

    def test_watcher_exits_on_plugin_disable(self):
        self.run_plugin("start")
        eventually(lambda: self.assertTrue((self.root / "reports").exists()))
        self.write("plugins.json", {"result": {"plugins": [{"plugin_id": "herdr-pr-status", "enabled": False}]}})
        eventually(self.watcher_exited)
        self.assertEqual(len(self.reports()), 1)

    def test_disable_during_lookup_stops_before_next_workspace(self):
        linked = self.root / "linked"
        self.git("worktree", "add", "-q", str(linked), "other")
        self.set_workspaces([self.workspace("w1", self.repo), self.workspace("w2", linked)])
        (self.root / "blocked").touch()
        self.run_plugin("start")
        eventually(lambda: self.assertTrue((self.root / "entered").exists()))
        self.write("plugins.json", {"result": {"plugins": []}})
        (self.root / "release").touch()
        eventually(self.watcher_exited)
        self.assertFalse((self.root / "reports").exists())
        self.assertEqual(len([args for kind, args in self.calls() if kind == "gh" and args[0] == "api"]), 1)

    def test_watcher_exits_on_plugin_unlink(self):
        self.run_plugin("start")
        eventually(lambda: self.assertTrue((self.root / "reports").exists()))
        self.write("plugins.json", {"result": {"plugins": []}})
        eventually(self.watcher_exited)
        self.assertEqual(len(self.reports()), 1)

    def test_watcher_refreshes_again_without_manual_action(self):
        self.run_plugin("start")
        eventually(lambda: self.assertTrue((self.root / "reports").exists()))
        self.write("prs.json", [pr(987)])
        eventually(lambda: self.assertEqual(len(self.reports()), 2), timeout=75)
        self.assertEqual(self.reports()[1][6], "pr_number=#987")
        self.run_plugin("stop")
        eventually(self.watcher_exited)

    def test_watcher_exits_when_socket_is_replaced(self):
        self.run_plugin("start")
        eventually(lambda: self.assertTrue((self.root / "reports").exists()))
        Path(self.env["HERDR_SOCKET_PATH"]).unlink()
        replacement = socket.socket(socket.AF_UNIX)
        self.addCleanup(replacement.close)
        replacement.bind(self.env["HERDR_SOCKET_PATH"])
        eventually(self.watcher_exited)
        self.assertEqual(len(self.reports()), 1)

    def test_watcher_exits_on_session_shutdown(self):
        self.run_plugin("start")
        eventually(lambda: self.assertTrue((self.root / "reports").exists()))
        Path(self.env["HERDR_SOCKET_PATH"]).unlink()
        eventually(self.watcher_exited)
        self.assertEqual(len(self.reports()), 1)

    def test_watcher_can_restart_immediately_after_stop(self):
        self.run_plugin("start")
        eventually(lambda: self.assertTrue((self.root / "reports").exists()))
        self.run_plugin("stop")
        self.run_plugin("start")
        eventually(lambda: self.assertEqual(len(self.reports()), 2))
        self.run_plugin("stop")
        eventually(lambda: self.assertEqual(next((self.root / "state").glob("*/watch.log")).read_text().count("PR watcher stopped"), 2))
        self.assertEqual(len(self.reports()), 2)

    def test_malformed_github_response_is_not_treated_as_no_pr(self):
        (self.root / "prs.json").write_text("invalid")
        result = self.run_plugin("refresh", check=False)
        self.assertEqual(result.returncode, 1)
        self.assertFalse((self.root / "reports").exists())

    def test_upstream_branch_name_is_used_for_github(self):
        self.git("config", "branch.feature.remote", "origin")
        self.git("config", "branch.feature.merge", "refs/heads/remote-feature")
        self.write("prs.json", [pr(333, branch="remote-feature")])
        self.run_plugin("refresh")
        self.assertEqual(self.reports()[0][6], "pr_number=#333")
        self.assertIn("head=alice:remote-feature", [args for kind, args in self.calls() if kind == "gh" and args[0] == "api"][0])

    def test_disabled_plugin_cannot_start(self):
        self.write("plugins.json", {"result": {"plugins": []}})
        result = self.run_plugin("start", check=False)
        self.assertEqual(result.returncode, 1)
        self.assertFalse(list((self.root / "state").glob("*/watch.log")))


if __name__ == "__main__":
    unittest.main(verbosity=2)
