"""Reject false-green shards and verify the frozen inputs of every recovery job."""

import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from artifact_download_outcome import classify
from shard_recovery import attempt_status, prepare, validate
from artifact_retention_policy import keep_ids

ROOT = Path(__file__).resolve().parents[2]


def read_yaml(path):
    """Use the same YAML parser available on the CI metadata runner."""
    return json.loads(subprocess.check_output([
        "ruby", "-ryaml", "-rjson", "-e", "puts JSON.generate(YAML.load_file(ARGV[0]))", str(path),
    ], text=True))


class DownloadClassification(unittest.TestCase):
    """Only three explicit transport failures may defer the required verdict."""

    def test_all_three_transfer_failures_are_required(self):
        self.assertEqual(classify(["failure"] * 3, ["skipped"] * 3, ["true"] * 3), "retryable-transfer")
        for flags in (["true", "", "true"], ["", "", ""], ["false"] * 3):
            self.assertEqual(classify(["failure"] * 3, ["skipped"] * 3, flags), "failure")

    def test_integrity_failure_cannot_trigger_another_runner(self):
        self.assertEqual(classify(["success", "failure", "failure"],
                                 ["failure", "skipped", "skipped"], ["true"] * 3), "failure")
        self.assertEqual(classify(["success"] * 3, ["failure"] * 3, ["true"] * 3), "failure")

    def test_complete_local_retry_still_accepts_valid_bytes(self):
        self.assertEqual(classify(["failure", "success", "skipped"],
                                 ["skipped", "success", "skipped"], ["true", "", ""]), "downloaded")
        self.assertEqual(classify(["success", "skipped", "skipped"], ["skipped"] * 3, [""] * 3), "downloaded")

    def test_unknown_cancelled_or_out_of_order_attempts_are_rejected(self):
        for downloads in (["cancelled", "skipped", "skipped"], ["skipped"] * 3,
                          ["skipped", "success", "skipped"], ["failure"]):
            with self.assertRaises(ValueError):
                classify(downloads, ["skipped"] * 3, ["true"] * 3)

    def test_deferral_is_explicitly_opted_into(self):
        with tempfile.TemporaryDirectory(prefix=".shard-outcome.", dir=ROOT) as tmp:
            for defer, expected in (("false", 1), ("true", 0)):
                env = {**os.environ, "DOWNLOAD_OUTCOMES": json.dumps(["failure"] * 3),
                       "VALIDATION_OUTCOMES": json.dumps(["skipped"] * 3),
                       "TRANSFER_FLAGS": json.dumps(["true"] * 3), "DEFER_TRANSFER_FAILURE": defer,
                       "GITHUB_OUTPUT": str(Path(tmp) / "output")}
                result = subprocess.run(["python3", "-B", "scripts/ci/artifact_download_outcome.py"],
                                        cwd=ROOT, env=env, capture_output=True, text=True)
                self.assertEqual(result.returncode, expected, result.stderr)


class RecoveryStates(unittest.TestCase):
    """Require tests to pass, and bound recovery to the transport-only state."""

    def test_recovery_runs_only_before_tests(self):
        self.assertEqual(prepare("retryable-transfer", "skipped", "curl"), "retryable-transfer")
        self.assertEqual(prepare("downloaded", "retryable-transfer", "curl"), "retryable-transfer")
        self.assertEqual(attempt_status("retryable-transfer", "skipped"), "retryable-transfer")
        self.assertEqual(attempt_status("ready", "success"), "passed")
        for prepared, tests in (("ready", "skipped"), ("ready", "failure"),
                                ("retryable-transfer", "success"), ("", "success")):
            with self.assertRaises(ValueError):
                attempt_status(prepared, tests)
        with self.assertRaises(ValueError):
            prepare("downloaded", "skipped", "curl")

    def test_only_two_verdict_combinations_can_pass(self):
        """Run the actual checkout-free workflow gate against every result combination."""
        workflow = read_yaml(ROOT / ".github/workflows/ci-test-shard.yml")
        script = workflow["jobs"]["verdict"]["steps"][0]["run"]

        def verdict(primary, recovery):
            """Execute the production gate rather than a second Python implementation."""
            result = subprocess.run(["bash", "-e", "-o", "pipefail", "-c", script],
                                    env={**os.environ, "ATTEMPTS": json.dumps({"primary": primary, "recovery": recovery})},
                                    capture_output=True, text=True)
            return result.returncode == 0

        results = ("success", "failure", "cancelled", "skipped", "")
        statuses = ("passed", "retryable-transfer", "", "ready")
        for first in results:
            for state in statuses:
                for second in results:
                    for recovered in statuses:
                        primary = {"result": first, "outputs": {"status": state}}
                        recovery = {"result": second, "outputs": {"status": recovered}}
                        expected = first == "success" and (
                            (state == "passed" and second == "skipped") or
                            (state == "retryable-transfer" and second == "success" and recovered == "passed"))
                        self.assertEqual(verdict(primary, recovery), expected, (primary, recovery))
        self.assertFalse(verdict({"result": "success"}, {"result": "skipped"}))

    @patch("shard_recovery.subprocess.check_output", return_value="a" * 40 + "\n")
    def test_checkout_and_producer_identity_are_frozen(self, _head):
        inputs = ["linux-x86_64", "codegen", "2", "32", "a" * 40, "123", "1",
                  "ghcr.io/illegalstudio/elephc-ci@sha256:" + "b" * 64]
        validate(*inputs)
        for index, value in ((0, "linux-other"), (2, "0"), (3, "1"), (4, "c" * 40),
                             (5, ""), (6, "0"), (7, "ghcr.io/illegalstudio/elephc-ci:latest")):
            changed = list(inputs)
            changed[index] = value
            with self.assertRaises(ValueError):
                validate(*changed)


class WorkflowRecovery(unittest.TestCase):
    """Check the actual orchestration, including its unchanged required-gate names."""

    @classmethod
    def setUpClass(cls):
        """Load the main coordinator, shared controller and common attempt action."""
        cls.main = read_yaml(ROOT / ".github/workflows/ci.yml")
        cls.workflow = read_yaml(ROOT / ".github/workflows/ci-test-shard.yml")
        cls.action = read_yaml(ROOT / ".github/actions/test-shard-attempt/action.yml")

    def test_primary_and_recovery_use_identical_inputs_and_setup(self):
        primary, recovery = (copy.deepcopy(self.workflow["jobs"][key]) for key in ("primary", "recovery"))
        self.assertEqual(recovery.pop("needs"), "primary")
        condition = recovery.pop("if")
        self.assertIn("!cancelled()", condition)
        self.assertIn("needs.primary.result == 'success'", condition)
        self.assertIn("needs.primary.outputs.status == 'retryable-transfer'", condition)
        primary.pop("name")
        recovery.pop("name")
        self.assertEqual(primary["steps"][-1]["with"].pop("execution"), "primary")
        self.assertEqual(recovery["steps"][-1]["with"].pop("execution"), "recovery")
        self.assertEqual(primary, recovery)
        self.assertEqual(primary["steps"][0]["with"]["ref"], "${{ inputs.source-sha }}")
        self.assertIn("inputs.ci-image", primary["container"]["image"])
        self.assertEqual(primary["env"]["ELEPHC_CI_IMAGE"], primary["container"]["image"])
        self.assertIn("startsWith(inputs.target, 'linux-')", primary["env"]["ELEPHC_CI_IMAGE"])

    def test_all_21_callers_keep_their_partitions_and_producers(self):
        families = {"non-codegen": (16, 16, 8), "cli": (6, 6, 6), "web": (3, 2, 2),
                    "codegen": (32, 32, 16), "pdo": (4, 4, 4), "curl": (4, 4, 4), "eval": (16, 16, 16)}
        targets = ("macos-aarch64", "linux-x86_64", "linux-aarch64")
        callers = [job for job in self.main["jobs"].values()
                   if job.get("uses") == "./.github/workflows/ci-test-shard.yml"]
        self.assertEqual(len(callers), 21)
        for job in callers:
            args = job["with"]
            count = families[args["family"]][targets.index(args["target"])]
            self.assertEqual(args["shards"], count)
            self.assertEqual(job["strategy"]["matrix"]["shard"], list(range(1, count + 1)))
            producer = "build-archive-" + args["target"]
            self.assertIn(producer, job["needs"])
            self.assertEqual(args["artifact-attempt"], "${{ needs." + producer + ".outputs.artifact-attempt }}")
            self.assertEqual(args["source-sha"], "${{ github.sha }}")
            self.assertEqual(args["artifact-run"], "${{ github.run_id }}")
            self.assertEqual(args["ci-image"], "${{ needs.plan.outputs.ci-image }}")
            self.assertIn(args["job-key"], self.main["jobs"]["test"]["needs"])

    def test_verdict_and_audit_cannot_be_skipped_after_a_failure(self):
        gate = self.workflow["jobs"]["verdict"]
        self.assertEqual(gate["if"], "always()")
        self.assertEqual(gate["needs"], ["primary", "recovery"])
        self.assertEqual(len(gate["steps"]), 1)
        self.assertIn("jq -e", gate["steps"][0]["run"])
        self.assertNotIn("uses", gate["steps"][0])
        audit = self.action["runs"]["steps"][-1]
        self.assertEqual(audit["if"], "always()")
        self.assertEqual(audit["uses"], "./.github/actions/assert-consumer-builds")
        self.assertNotIn("continue-on-error", json.dumps(self.workflow))

    def test_active_recovery_preserves_inputs_until_the_workflow_finishes(self):
        """The existing janitor must retain every input while the verdict is pending."""
        run = {"id": 123, "path": ".github/workflows/ci.yml", "status": "in_progress",
               "repository": {"full_name": "illegalstudio/elephc"}, "event": "merge_group",
               "head_branch": "gh-readonly-queue/main/pr-1", "conclusion": None}
        artifacts = [{"id": 1}, {"id": 2}]
        args = ([], artifacts, None, "illegalstudio/elephc", [], set(), True, 0)
        self.assertEqual(keep_ids(run, *args), {1, 2})
        run.update(status="completed", conclusion="failure")
        self.assertEqual(keep_ids(run, *args), set())


if __name__ == "__main__":
    unittest.main()
