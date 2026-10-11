"""Focused tests for builtin backend-support and PHP-availability target policy."""

import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "audit_builtin_eir_boundary.py"
SPEC = importlib.util.spec_from_file_location("audit_builtin_eir_boundary", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class TargetPolicyTests(unittest.TestCase):
    """Pins independent policy expansion and exact effective intersections."""

    def test_all_backend_intersects_unix_php_availability(self):
        """Backend `all` does not make a Unix-only PHP symbol available on Windows."""
        self.assertEqual(
            MODULE.effective_targets("all", "unix"),
            [
                "macos-aarch64",
                "ios-arm64",
                "ios-sim-arm64",
                "linux-aarch64",
                "linux-x86_64",
            ],
        )

    def test_all_backend_intersects_windows_php_availability(self):
        """Windows-only PHP availability selects only the Windows backend target."""
        self.assertEqual(
            MODULE.effective_targets("all", "windows"), ["windows-x86_64"]
        )

    def test_host_only_backend_intersects_all_php_availability(self):
        """Host-only backend support excludes both iOS library targets."""
        self.assertEqual(
            MODULE.effective_targets("host_only", "all"), MODULE.HOST_ONLY_TARGETS
        )

    def test_unix_backend_intersects_windows_to_empty(self):
        """Disjoint backend and PHP policies have no effective targets."""
        self.assertEqual(MODULE.effective_targets("unix", "windows"), [])

    def test_unknown_policy_fails_closed(self):
        """Unknown policy spellings cannot silently become an empty verified set."""
        with self.assertRaisesRegex(ValueError, "unknown target policy"):
            MODULE.effective_targets("all", "future_platform")


if __name__ == "__main__":
    unittest.main()
