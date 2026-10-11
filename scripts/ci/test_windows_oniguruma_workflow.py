"""Regression checks for the Windows managed-Oniguruma CI bundle."""

import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/ci.yml"
PREPARE = ROOT / "scripts/ci/prepare_oniguruma_tests.py"


def load_prepare_module():
    """Load the preparation script without changing its production entry point."""
    spec = importlib.util.spec_from_file_location("prepare_oniguruma_tests", PREPARE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class WindowsOnigurumaWorkflowTests(unittest.TestCase):
    """Keep the native Windows recipe genuine while its process cwd stays short."""

    @classmethod
    def setUpClass(cls):
        cls.workflow = WORKFLOW.read_text(encoding="utf-8")
        start = cls.workflow.index("      - name: Cache managed Windows Oniguruma")
        end = cls.workflow.index("      - name: Execute the complete PE suite", start)
        cls.section = cls.workflow[start:end]

    def test_bundle_uses_short_runner_temp_path(self):
        self.assertIn("path: ${{ runner.temp }}/eo", self.section)
        self.assertIn("key: managed-oniguruma-windows-x86_64-short-v2-", self.section)
        self.assertIn("'scripts/ci/prepare_oniguruma_tests.py'", self.section)
        self.assertIn('"$RUNNER_TEMP/eo"', self.section)
        self.assertIn('--cache "$RUNNER_TEMP/eo/c"', self.section)
        self.assertIn('ELEPHC_NATIVE_CACHE="$RUNNER_TEMP/eo/c"', self.section)
        self.assertNotIn(".ci/elephc-oniguruma-windows-x86_64", self.section)

    def test_bundle_uses_production_recipe_and_windows_target(self):
        self.assertIn("python scripts/ci/prepare_oniguruma_tests.py", self.section)
        self.assertIn("target/debug/elephc.exe", self.section)
        self.assertIn("--target windows-x86_64", self.section)
        self.assertIn("native prune --target windows-x86_64", self.section)
        self.assertIn("project/elephc.lock", self.section)
        self.assertIn("libelephc_oniguruma_shim.a", self.section)


class PrepareOnigurumaTests(unittest.TestCase):
    """Verify cache selection without downloading or replacing the production recipe."""

    def run_prepare(self, cache_override):
        module = load_prepare_module()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            compiler = root / "elephc.exe"
            compiler.touch()
            bundle = root / "bundle"
            cache = cache_override(root, bundle)
            calls = []

            def fake_run(argv, *, env, check):
                calls.append((argv, env.copy(), check))
                (bundle / "project").mkdir(parents=True, exist_ok=True)
                (bundle / "project/elephc.lock").touch()
                (cache / "sources").mkdir(parents=True, exist_ok=True)
                (cache / "sources/oniguruma.tar.gz").touch()

            argv = [
                str(PREPARE),
                str(compiler),
                str(bundle),
                "--target",
                "windows-x86_64",
            ]
            if cache != bundle / "cache":
                argv.extend(["--cache", str(cache)])
            with mock.patch.object(sys, "argv", argv), mock.patch.object(
                module.subprocess, "run", side_effect=fake_run
            ):
                module.main()
            return calls, cache.resolve(), bundle.resolve()

    def assert_production_commands(self, calls, cache, bundle):
        self.assertEqual(len(calls), 2)
        add, install = calls
        self.assertEqual(add[0][1:4], ["native", "add", "oniguruma"])
        self.assertEqual(install[0][1:4], ["native", "install", "--locked"])
        for argv, env, check in calls:
            self.assertTrue(check)
            self.assertEqual(Path(env["ELEPHC_NATIVE_CACHE"]), cache)
            self.assertIn("--target", argv)
            self.assertEqual(argv[argv.index("--target") + 1], "windows-x86_64")
            self.assertIn("--manifest-path", argv)
            self.assertEqual(
                Path(argv[argv.index("--manifest-path") + 1]),
                bundle / "project/elephc.toml",
            )
        self.assertIn("--offline", install[0])
        self.assertNotIn("--offline", add[0])

    def test_default_cache_remains_inside_bundle(self):
        calls, cache, bundle = self.run_prepare(lambda _root, bundle: bundle / "cache")
        self.assertEqual(cache, bundle / "cache")
        self.assert_production_commands(calls, cache, bundle)

    def test_cache_override_is_forwarded_to_both_native_commands(self):
        calls, cache, bundle = self.run_prepare(lambda root, _bundle: root / "short-cache")
        self.assertEqual(cache.name, "short-cache")
        self.assert_production_commands(calls, cache, bundle)


if __name__ == "__main__":
    unittest.main()
