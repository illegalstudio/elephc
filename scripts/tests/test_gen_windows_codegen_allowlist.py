"""Regression tests for strict native Windows codegen coverage verification."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / "scripts" / "gen_windows_codegen_allowlist.py"


class WindowsCodegenCoverageTests(unittest.TestCase):
    """Exercises strict native shard coverage through the public CLI."""

    def test_baseline_commands_are_not_exposed(self) -> None:
        """Keeps allow-list generation and failure exemptions out of the strict gate."""
        for removed_command in ("generate", "gate"):
            result = subprocess.run(
                [sys.executable, str(SCRIPT), removed_command],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("invalid choice", result.stderr)

    def run_complete(
        self,
        runnable: list[str],
        shard_cases: list[list[str]],
        failed: set[str] | None = None,
        require_success: bool = False,
        expected_junit_count: int | None = None,
    ) -> subprocess.CompletedProcess[str]:
        """Runs aggregate coverage verification for synthetic shard reports."""
        failed = failed or set()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            list_path = root / "list.json"
            testcases = {
                name: {"ignored": False, "filter-match": {"status": "matches"}}
                for name in runnable
            }
            list_path.write_text(
                json.dumps(
                    {
                        "rust-suites": {
                            "elephc::codegen_tests": {"testcases": testcases}
                        }
                    }
                ),
                encoding="utf-8",
            )
            reports: list[Path] = []
            for index, names in enumerate(shard_cases):
                report = root / f"shard-{index}.xml"
                cases = "".join(
                    (
                        f'<testcase name="{name}"><failure /></testcase>'
                        if name in failed
                        else f'<testcase name="{name}" />'
                    )
                    for name in names
                )
                report.write_text(
                    f'<testsuites><testsuite>{cases}</testsuite></testsuites>',
                    encoding="utf-8",
                )
                reports.append(report)

            command = [
                sys.executable,
                str(SCRIPT),
                "verify-complete",
                "--list-json",
                str(list_path),
            ]
            for report in reports:
                command.extend(["--junit", str(report)])
            if require_success:
                command.append("--require-success")
            if expected_junit_count is not None:
                command.extend(
                    ["--expected-junit-count", str(expected_junit_count)]
                )
            return subprocess.run(
                command, check=False, capture_output=True, text=True
            )

    def test_complete_shards_cover_runnable_set_once(self) -> None:
        """Accepts an exact one-time partition of the runnable fixtures."""
        result = self.run_complete(
            ["codegen::a", "codegen::b"],
            [["codegen::a"], ["codegen::b"]],
            expected_junit_count=2,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("exactly cover", result.stdout)

    def test_wrong_junit_report_count_is_rejected(self) -> None:
        """Rejects an aggregate whose report count differs from the CI shard count."""
        result = self.run_complete(
            ["codegen::a"],
            [["codegen::a"]],
            expected_junit_count=16,
        )

        self.assertEqual(result.returncode, 1)
        self.assertIn("expected 16 JUnit report(s), found 1", result.stderr)

    def test_incomplete_shards_are_rejected(self) -> None:
        """Rejects a truncated aggregate report even when all observed tests pass."""
        result = self.run_complete(["codegen::a", "codegen::b"], [["codegen::a"]])

        self.assertEqual(result.returncode, 1)
        self.assertIn("missing testcase(s): 1", result.stderr)
        self.assertIn("codegen::b", result.stderr)

    def test_duplicate_shard_case_is_rejected(self) -> None:
        """Rejects overlapping partitions that execute one fixture twice."""
        result = self.run_complete(["codegen::a"], [["codegen::a"], ["codegen::a"]])

        self.assertEqual(result.returncode, 1)
        self.assertIn("duplicated testcase(s): 1", result.stderr)

    def test_strict_complete_verification_rejects_failures(self) -> None:
        """Rejects a complete native Windows run when any fixture failed."""
        result = self.run_complete(
            ["codegen::a", "codegen::b"],
            [["codegen::a"], ["codegen::b"]],
            failed={"codegen::b"},
            require_success=True,
        )

        self.assertEqual(result.returncode, 1)
        self.assertIn("failed testcase(s): 1", result.stderr)
        self.assertIn("codegen::b", result.stderr)


if __name__ == "__main__":
    unittest.main()
