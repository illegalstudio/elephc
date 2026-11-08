"""Execute the real workflow gate scripts against failure, cancellation and reuse cases."""

import copy
import json
import os
import re
import subprocess
import textwrap
import unittest
from pathlib import Path

from test_ci_artifact_graph import dependencies, jobs
from artifact_attempts import select_attempts

ROOT = Path(__file__).resolve().parents[2]


def script(body):
    """The verdict is the last run block in each aggregate job."""
    value = body.rsplit("        run: |\n", 1)[1]
    return textwrap.dedent(value)


class AggregateGateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.jobs = jobs((ROOT / ".github/workflows/ci.yml").read_text())

    def execute(self, gate, results, **extra):
        env = dict(os.environ, RESULTS=json.dumps(results), NEEDS=json.dumps(results), **extra)
        return subprocess.run(["bash", "-e", "-o", "pipefail", "-c", script(self.jobs[gate])],
                              env=env, capture_output=True, text=True).returncode

    def build_results(self, run="true", extended="true", drivers="true", benchmark="false"):
        results = {name: {"result": "success", "outputs": {"artifact-attempt": "1"}}
                   for name in dependencies(self.jobs["build"])}
        results["plan"]["outputs"] = {"run": run, "extended": extended,
                                        "pdo-drivers": drivers, "benchmark": benchmark}
        for name in results:
            if name == "plan":
                continue
            selected = {"build-ios": extended, "build-pdo-drivers": drivers,
                        "build-benchmark": benchmark}.get(name, run)
            results[name]["result"] = "success" if selected == "true" else "skipped"
        return results

    def test_every_planned_producer_failure_cancellation_or_skip_is_red(self):
        results = self.build_results(benchmark="true")
        self.assertEqual(self.execute("build", results), 0)
        for name in results:
            for failure in ("failure", "cancelled", "skipped"):
                changed = copy.deepcopy(results)
                changed[name]["result"] = failure
                self.assertNotEqual(self.execute("build", changed), 0, (name, failure))

    def test_main_reuse_and_narrow_pr_accept_only_the_planned_skips(self):
        self.assertEqual(self.execute("build", self.build_results(extended="false", drivers="false")), 0)
        results = self.build_results(run="false", extended="false", drivers="false", benchmark="true")
        self.assertEqual(self.execute("build", results), 0)
        results["build-benchmark"]["result"] = "skipped"
        self.assertNotEqual(self.execute("build", results), 0)

    def test_both_pdo_gates_reject_failed_cancelled_and_unexpected_skipped_dependencies(self):
        for gate in ("pdo-core-gate", "pdo-drivers-gate", "phpext-host-sdk"):
            results = {name: {"result": "success"} for name in dependencies(self.jobs[gate])}
            self.assertEqual(self.execute(gate, results, SELECTED="true"), 0)
            for name in results:
                for failure in ("failure", "cancelled", "skipped"):
                    changed = copy.deepcopy(results)
                    changed[name]["result"] = failure
                    self.assertNotEqual(self.execute(gate, changed, SELECTED="true"), 0)
            for name in results:
                if name != "plan":
                    results[name]["result"] = "skipped"
            self.assertEqual(self.execute(gate, results, SELECTED="false"), 0)

    def test_build_and_test_fails_on_a_failed_build_or_unknown_dependency(self):
        results = {name: {"result": "success"} for name in dependencies(self.jobs["test"])}
        self.assertEqual(self.execute("test", results, RUN_TIER="true", EXTENDED_TIER="true"), 0)
        for failure in ("failure", "cancelled", "skipped"):
            results["build"]["result"] = failure
            self.assertNotEqual(self.execute("test", results, RUN_TIER="true", EXTENDED_TIER="true"), 0)
        results["build"]["result"] = "success"
        results["unclassified-new-suite"] = {"result": "success"}
        self.assertNotEqual(self.execute("test", results, RUN_TIER="true", EXTENDED_TIER="true"), 0)

    def test_partial_matrix_rerun_retains_each_original_producer_attempt(self):
        artifacts = [{"name": f"build-{target}-debug-ios-default-{attempt}",
                      "expired": False, "size_in_bytes": 1}
                     for target, attempt in (("ios-arm64", 1), ("ios-sim-arm64", 1), ("ios-sim-arm64", 2))]
        self.assertEqual(select_attempts(artifacts, ["ios-arm64", "ios-sim-arm64"], "ios"),
                         {"ios-arm64": "1", "ios-sim-arm64": "2"})
        with self.assertRaises(ValueError):
            select_attempts(artifacts[:1], ["ios-arm64", "ios-sim-arm64"], "ios")


if __name__ == "__main__":
    unittest.main()
