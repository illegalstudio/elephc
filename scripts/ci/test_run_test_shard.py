"""Preserve archived test filters, platform workers and specialized shard helpers."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from run_test_shard import run

ROOT = Path(__file__).resolve().parents[2]


class ShardExecution(unittest.TestCase):
    """Inspect real command plans without compiling or launching the Rust suites."""

    def setUp(self):
        """Make an isolated archive-shaped executable directory for helper discovery."""
        tmp = tempfile.TemporaryDirectory(prefix=".shard-execution.", dir=ROOT)
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        binary = self.root / "nextest/target/debug/deps/codegen_tests-fixture"
        binary.parent.mkdir(parents=True)
        binary.write_text("#!/bin/sh\nexit 0\n")
        binary.chmod(0o755)
        self.calls = []

    def execute(self, family, target, shard="1", count="4"):
        """Capture the exact production argument vectors and per-command environment."""
        with patch("run_test_shard.execute", side_effect=lambda args, env: self.calls.append(
                ([str(arg) for arg in args], dict(env)))), patch("run_test_shard.shutil.which", return_value="/tools/tool"):
            run(family, target, shard, count, self.root, {"RUST_MIN_STACK": "33554432"})

    def test_normal_filters_and_worker_counts_match_each_platform(self):
        for target in ("macos-aarch64", "linux-x86_64", "linux-aarch64"):
            for family, marker, workers in (("non-codegen", "not binary(codegen_tests)", 3 if target.startswith("macos") else 4),
                                            ("cli", "binary(/^(opcache_|eval_|php_profile_)/)", 2),
                                            ("web", "binary(web_tests) or binary(web_session_tests)", 1)):
                self.calls.clear()
                self.execute(family, target, "2", "6")
                self.assertEqual(len(self.calls), 1)
                args, env = self.calls[0]
                self.assertIn(marker, args[args.index("-E") + 1])
                self.assertEqual(args[args.index("--partition") + 1], "hash:2/6")
                self.assertEqual(args[args.index("-j") + 1], str(workers))
                self.assertEqual(args[args.index("--archive-file") + 1], str(self.root / "nextest-archive.tar.zst"))
                self.assertEqual(env["RUST_MIN_STACK"], "33554432")

    def test_codegen_keeps_native_requirements_and_excludes_separate_families(self):
        for target in ("macos-aarch64", "linux-x86_64", "linux-aarch64"):
            self.calls.clear()
            self.execute("codegen", target, "3", "32")
            self.assertIn("--no-run", self.calls[0][0])
            native = [args for args, _ in self.calls if "--manifest-path" in args]
            self.assertEqual([args[-1] for args in native], [f"examples/{name}/elephc.toml"
                             for name in ("xml", "gmp", "openssl-cipher", "fileinfo")])
            self.assertTrue(all(args[args.index("--target") + 1] == target for args in native))
            args, env = self.calls[-1]
            query = args[args.index("-E") + 1]
            for family in ("eval", "pdo", "curl"):
                self.assertIn(f"not test(~codegen::{family})", query)
            for package in ("XML", "GMP", "FILEINFO", "OPENSSL"):
                self.assertEqual(env[f"ELEPHC_TEST_REQUIRE_{package}_NATIVE"], "1")
            self.assertEqual(env["ELEPHC_TEST_PREBUILT_BRIDGES"], "1")
            self.assertEqual(args[-1], "3" if target.startswith("macos") else "4")

    def test_single_process_helpers_keep_their_archive_and_environment(self):
        for family, count in (("pdo", "4"), ("curl", "4"), ("eval", "16")):
            self.calls.clear()
            self.execute(family, "linux-aarch64", "2", count)
            args, env = self.calls[-1]
            self.assertEqual(args[:2], ["bash", f"scripts/ci/run_{family}_codegen_shard.sh"])
            self.assertEqual(args[-2:], ["2", count])
            self.assertEqual(env["CARGO_NET_OFFLINE"], "true")
            if family == "eval":
                self.assertEqual(env["EVAL_SHARD_NEXTEST_BIN"], str(self.root / "cargo-nextest"))
                self.assertEqual(env["EVAL_SHARD_JUNIT_FILE"], "target/nextest/ci/junit-pass1.xml")
            if family == "curl":
                self.assertEqual(env["ELEPHC_TEST_BINARY_TIMEOUT_SECS"], "120")
                install = next(args for args, _ in self.calls if "--manifest-path" in args)
                self.assertIn("--offline", install)

    def test_failed_setup_or_test_is_propagated_without_retry(self):
        with patch("run_test_shard.execute", side_effect=subprocess.CalledProcessError(7, "test")) as execute:
            with self.assertRaises(subprocess.CalledProcessError):
                run("eval", "linux-x86_64", "1", "16", self.root, os.environ)
        self.assertEqual(execute.call_count, 1)

    def test_missing_native_tools_use_the_checked_apt_wrapper(self):
        """Older CI images must install m4/gdb through the same bounded provisioning."""
        with patch("run_test_shard.execute", side_effect=lambda args, env: self.calls.append(
                ([str(arg) for arg in args], dict(env)))), patch("run_test_shard.shutil.which",
                side_effect=lambda tool: "/usr/bin/apt-get" if tool == "apt-get" else None):
            run("codegen", "linux-x86_64", "2", "32", self.root, {})
        installs = [args for args, _ in self.calls if "scripts/ci/apt_install.sh" in args]
        self.assertEqual(installs, [["bash", "scripts/ci/apt_install.sh", "--no-install-recommends", package]
                                   for package in ("m4", "gdb")])


if __name__ == "__main__":
    unittest.main()
