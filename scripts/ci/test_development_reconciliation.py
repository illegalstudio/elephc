"""Exercise skipped events, checkpoint crash recovery and bounded backfill policy."""

import copy
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import reconcile_development as subject

SHA = "a" * 40
OTHER = "b" * 40
NOW = 100000


class ReconciliationTests(unittest.TestCase):
    def test_rollout_excludes_old_history_and_records_itself(self):
        ledger = subject.initial_ledger(SHA, NOW)
        self.assertEqual(ledger["checkpoint"], SHA)
        self.assertEqual(list(ledger["pending"]), [SHA])

    def test_multiple_main_commits_have_distinct_durable_requests(self):
        ledger = subject.initial_ledger(SHA, NOW)
        commits = [f"{number:040x}" for number in range(25)]
        subject.record_page(ledger, commits, NOW)
        self.assertEqual(ledger["checkpoint"], commits[19])
        self.assertEqual(len(ledger["pending"]), 21)
        subject.record_page(ledger, commits[:20], NOW + 10)
        self.assertEqual(len(ledger["pending"]), 21)
        subject.record_page(ledger, commits[20:], NOW + 10)
        self.assertEqual(ledger["checkpoint"], commits[-1])

    def test_expired_packages_prove_previous_production_only_for_exact_sha(self):
        artifacts = [{"name": f"elephc-dev-{target}-{SHA}-1", "expired": True,
                      "size_in_bytes": 100} for target in subject.TARGETS]
        self.assertTrue(subject.produced(SHA, artifacts))
        self.assertFalse(subject.produced(OTHER, artifacts))
        self.assertFalse(subject.produced(SHA, artifacts[:2]))

    def test_inflight_runs_and_retry_budget_prevent_dispatch_storms(self):
        record = {"attempts": 0}
        self.assertTrue(subject.dispatch_due(record, NOW))
        self.assertFalse(subject.dispatch_due(record, NOW, NOW - 60))
        self.assertTrue(subject.dispatch_due(record, NOW, NOW - 21601))
        self.assertFalse(subject.dispatch_due({"attempts": 3}, NOW))
        self.assertFalse(subject.dispatch_due({"attempts": 1, "last_dispatch": NOW - 100}, NOW))

    def test_source_must_be_full_sha_on_post_rollout_main_chain(self):
        with patch.object(subject, "rollout", return_value=SHA), patch.object(subject, "git", return_value=OTHER):
            self.assertEqual(subject.validate_source(SHA), SHA)
            self.assertEqual(subject.validate_source(OTHER), OTHER)
            for invalid in ("main", "a" * 7, "c" * 40):
                with self.assertRaises(ValueError):
                    subject.validate_source(invalid)

    def test_real_main_history_rejects_pr_heads_and_pre_rollout_commits(self):
        """The dispatch boundary must resolve real first-parent history, including merges."""
        with tempfile.TemporaryDirectory(prefix="elephc-main-history-") as directory:
            root = Path(directory)

            def git(*args):
                return subprocess.check_output(
                    ["git", "-C", directory, "-c", "commit.gpgsign=false",
                     "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", *args],
                    text=True, stderr=subprocess.DEVNULL).strip()

            def commit(message):
                git("commit", "--allow-empty", "-m", message)
                return git("rev-parse", "HEAD")

            git("init", "--initial-branch=main")
            before = commit("test: establish pre-rollout history")
            git("checkout", "-b", "rollout")
            workflow = root / ".github/workflows/development-builds.yml"
            workflow.parent.mkdir(parents=True)
            workflow.write_text("name: development artifacts\n")
            git("add", ".")
            pr_head = commit("test: introduce development artifacts")
            git("checkout", "main")
            git("merge", "--no-ff", "rollout", "-m", "test: merge rollout")
            anchor = git("rev-parse", "HEAD")
            git("checkout", "-b", "unmerged")
            unmerged = commit("test: create an unmerged candidate")
            git("checkout", "main")
            current = commit("test: advance main")
            git("update-ref", "refs/remotes/origin/main", current)
            with patch.object(subject, "git", side_effect=git):
                self.assertEqual(subject.rollout(), anchor)
                for allowed in (anchor, current):
                    self.assertEqual(subject.validate_source(allowed), allowed)
                for forbidden in (before, pr_head, unmerged):
                    with self.assertRaises(ValueError):
                        subject.validate_source(forbidden)

    def test_crash_at_dispatch_keeps_both_request_and_advanced_checkpoint(self):
        class Crash(subject.Reconciler):
            def load(self):
                return subject.initial_ledger(SHA, NOW)

            def save(self, ledger):
                self.saved = copy.deepcopy(ledger)

            def pages(self, *args):
                return []

            def observe(self, *args):
                return None, None

            def api(self, *args):
                if "per_page=100" in args[0]:
                    return {"workflow_runs": []}
                raise ConnectionError("response lost after dispatch")

        worker = Crash("test/repo", NOW)
        with patch.object(subject, "git", return_value=OTHER), patch.object(subprocess, "run"):
            with self.assertRaises(ConnectionError):
                worker.reconcile()
        self.assertEqual(worker.saved["checkpoint"], OTHER)
        self.assertEqual(worker.saved["pending"][SHA]["attempts"], 1)
        self.assertIn(OTHER, worker.saved["pending"])
        self.assertFalse(subject.dispatch_due(worker.saved["pending"][SHA], NOW + 60))

    def test_missing_ci_event_dispatches_at_most_three_requests(self):
        class Worker(subject.Reconciler):
            calls = []

            def load(self):
                return subject.initial_ledger(SHA, NOW)

            def save(self, ledger):
                self.saved = copy.deepcopy(ledger)

            def pages(self, *args):
                return []

            def observe(self, *args):
                return None, None

            def api(self, *args):
                if "per_page=100" in args[0]:
                    return {"workflow_runs": []}
                self.calls.append(args)

        worker = Worker("test/repo", NOW)
        with patch.object(subject, "git", return_value="\n".join(f"{n:040x}" for n in range(30))), patch.object(subprocess, "run"), patch("builtins.print"):
            worker.reconcile()
        self.assertEqual(len(worker.calls), 3)
        self.assertEqual(len(worker.saved["pending"]), 21)
        self.assertEqual(worker.saved["checkpoint"], f"{19:040x}")


if __name__ == "__main__":
    unittest.main()
