#!/usr/bin/env python3
"""Regression cases for queue provenance, incomplete uploads and partial reruns."""

import copy
import unittest

from resolve_main_build import GATES, HOSTS, select
from verify_ci_verdicts import successful_latest, trusted_verdict_run


class MainBuildReuseTests(unittest.TestCase):
    """A successful build alone is never sufficient evidence to skip tests."""

    def setUp(self):
        self.sha = "a" * 40
        self.run = {"id": 123, "head_sha": self.sha, "event": "merge_group",
                    "path": ".github/workflows/ci.yml", "repository": {"full_name": "o/r"},
                    "status": "in_progress", "conclusion": None, "run_attempt": 3}
        self.jobs = [{"name": name, "conclusion": "success"} for name in GATES]
        names = []
        for target in HOSTS:
            names += [f"build-{target}-debug-{kind}-default-1" for kind in ("compiler", "bridges")]
        self.artifacts = [{"name": name, "size_in_bytes": 10, "expired": False,
                           "workflow_run": {"head_sha": self.sha}} for name in names]

    def selected(self):
        return select(self.run, self.jobs, self.artifacts, "o/r", self.sha)

    def test_required_gates_can_finish_before_optional_work(self):
        self.assertEqual(self.selected()["run_id"], "123")
        self.assertEqual(self.selected()["attempts"], dict.fromkeys(HOSTS, "1"))

    def test_partial_rerun_keeps_original_producer_attempt(self):
        self.jobs.append({"name": "Unrelated optional job", "conclusion": None})
        self.assertEqual(self.run["run_attempt"], 3)
        self.assertEqual(self.selected()["attempts"]["linux-x86_64"], "1")

    def test_disposable_archives_are_not_needed_after_every_required_gate_passes(self):
        """Cleaning completed test inputs must not cause main to rebuild a green queue SHA."""
        self.assertEqual(len(self.artifacts), 6)
        self.assertIsNotNone(self.selected())
        self.jobs[1]["conclusion"] = "failure"
        self.assertIsNone(self.selected())

    def test_pr_head_other_queue_entry_and_headgreen_intermediate_are_not_proof(self):
        for field, value in (("event", "pull_request"), ("head_sha", "b" * 40),
                             ("repository", {"full_name": "fork/r"}),
                             ("path", ".github/workflows/pdo-live.yml")):
            with self.subTest(field=field):
                original = copy.deepcopy(self.run)
                self.run[field] = value
                self.assertIsNone(self.selected())
                self.run = original
        # HEADGREEN does not transfer the integrated tip's verdict to an ancestor.
        self.assertIsNone(select(self.run, self.jobs, self.artifacts, "o/r", "c" * 40))

    def test_both_pdo_verdicts_are_required_in_the_coordinator(self):
        for index in range(len(GATES)):
            with self.subTest(gate=GATES[index]):
                jobs = self.jobs[:index] + self.jobs[index + 1:]
                self.assertIsNone(select(self.run, jobs, self.artifacts, "o/r", self.sha))

    def test_failed_cancelled_skipped_and_nested_gates_are_rejected(self):
        for conclusion in ("failure", "cancelled", "skipped", None):
            for gate in self.jobs:
                old = gate["conclusion"]
                gate["conclusion"] = conclusion
                self.assertIsNone(self.selected())
                gate["conclusion"] = old
        self.jobs[0]["name"] = "child / Build & Test"
        self.assertIsNone(self.selected())

    def test_missing_expired_empty_and_wrong_sha_artifacts_rebuild(self):
        for index in range(len(self.artifacts)):
            with self.subTest(artifact=self.artifacts[index]["name"]):
                artifacts = self.artifacts[:index] + self.artifacts[index + 1:]
                self.assertIsNone(select(self.run, self.jobs, artifacts, "o/r", self.sha))
        for field, value in (("expired", True), ("size_in_bytes", 0),
                             ("workflow_run", {"head_sha": "b" * 40})):
            artifacts = copy.deepcopy(self.artifacts)
            artifacts[0][field] = value
            self.assertIsNone(select(self.run, self.jobs, artifacts, "o/r", self.sha))

    def test_attempts_cannot_be_mixed_with_an_incomplete_new_build(self):
        self.artifacts[1]["name"] = self.artifacts[1]["name"].removesuffix("1") + "2"
        self.assertIsNone(self.selected())

    def test_cancelled_or_failed_producer_run_is_rejected(self):
        for conclusion in ("cancelled", "failure", "timed_out", "action_required"):
            self.run["conclusion"] = conclusion
            self.assertIsNone(self.selected())


class ReleaseVerdictTests(unittest.TestCase):
    def test_publication_requires_full_integration_event_not_a_green_pr_ancestor(self):
        run = {"head_sha": "a" * 40, "repository": {"full_name": "o/r"},
               "path": ".github/workflows/ci.yml", "event": "merge_group",
               "head_branch": "gh-readonly-queue/main/pr-1-a", "conclusion": "success"}
        self.assertTrue(trusted_verdict_run(run, "o/r", "a" * 40))
        self.assertTrue(trusted_verdict_run({**run, "event": "push", "head_branch": "main"}, "o/r", "a" * 40))
        for changes in ({"event": "pull_request"}, {"head_sha": "b" * 40},
                        {"event": "push", "head_branch": "feature"}, {"conclusion": "cancelled"},
                        {"repository": {"full_name": "fork/r"}}, {"path": ".github/workflows/development-builds.yml"}):
            self.assertFalse(trusted_verdict_run({**run, **changes}, "o/r", "a" * 40))

    def test_latest_verdict_cannot_hide_a_pending_or_failed_rerun(self):
        check = {"id": 1, "name": GATES[0], "head_sha": "a" * 40,
                 "app": {"slug": "github-actions"}, "status": "completed", "conclusion": "success"}
        self.assertTrue(successful_latest([check], GATES[0], "a" * 40))
        self.assertFalse(successful_latest([check], GATES[0], "b" * 40))
        for state, conclusion in (("in_progress", None), ("completed", "failure"), ("completed", "cancelled")):
            newer = {**check, "id": 2, "status": state, "conclusion": conclusion}
            self.assertFalse(successful_latest([newer, check], GATES[0], "a" * 40))


if __name__ == "__main__":
    unittest.main()
