"""Verify archived PDO suites preserve one-process libtest selection and fail closed."""

import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import run_prebuilt_codegen_tests as subject


class PrebuiltCodegenTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="elephc-libtest-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.binary = self.root / "target/debug/deps/codegen_tests-fixture"
        self.binary.parent.mkdir(parents=True)
        self.binary.write_text("#!/bin/sh\nexit 0\n")
        self.binary.chmod(0o755)
        self.environment = patch.dict(os.environ, {"ELEPHC_CI_TEST_ROOT": str(self.root)})
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def test_selected_suite_runs_once_with_original_arguments_and_relocated_paths(self):
        args = ["mysql", "--ignored", "--test-threads=4"]
        with patch.object(subject.subprocess, "check_output", return_value="a: test\nb: test\n") as listing, \
                patch.object(subject.subprocess, "call", return_value=17) as execute:
            self.assertEqual(subject.run(args), 17)
        self.assertEqual(listing.call_args.args[0], [str(self.binary), *args, "--list"])
        execute.assert_called_once()
        self.assertEqual(execute.call_args.args[0], [str(self.binary), *args])
        environment = execute.call_args.kwargs["env"]
        self.assertEqual(environment["CARGO_TARGET_DIR"], str(self.root / "target"))
        self.assertEqual(environment["ELEPHC_TEST_PREBUILT_BRIDGES"], "1")
        self.assertEqual(environment["CARGO_MANIFEST_DIR"], os.getcwd())

    def test_empty_selection_never_becomes_a_green_run(self):
        with patch.object(subject.subprocess, "check_output", return_value="0 tests, 0 benchmarks\n"), \
                patch.object(subject.subprocess, "call") as execute:
            with self.assertRaises(ValueError):
                subject.run(["nonexistent", "--ignored"])
            execute.assert_not_called()

    def test_missing_or_ambiguous_binary_never_selects_arbitrary_bytes(self):
        self.binary.unlink()
        with self.assertRaises(ValueError):
            subject.run(["mysql"])
        for name in ("codegen_tests-one", "codegen_tests-two"):
            path = self.binary.with_name(name)
            path.write_text("fixture")
            path.chmod(0o755)
        with self.assertRaises(ValueError):
            subject.run(["mysql"])

    def test_a_filter_matching_only_ignored_tests_fails_without_the_opt_in(self):
        with patch.object(subject.subprocess, "check_output", return_value="ignored_case: test\n"), \
                patch.object(subject.subprocess, "call") as execute:
            with self.assertRaises(ValueError):
                subject.run(["ignored_case"])
            execute.assert_not_called()

if __name__ == "__main__":
    unittest.main()
