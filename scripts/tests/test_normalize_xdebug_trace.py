"""Regression tests for generic Xdebug format-4 trace normalization."""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "normalize_xdebug_trace.py"
SPEC = importlib.util.spec_from_file_location("normalize_xdebug_trace", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


TRACE = """Version: 3.5.0
File format: 4
TRACE START [2026-01-01 00:00:00.000000]
1\t0\t0\t0.0\t0\t{main}\t1\t\t/app/index.php\t0
2\t1\t0\t0.0\t0\trequire_once\t1\t/app/vendor/autoload.php\t/app/index.php\t5
3\t2\t0\t0.0\t0\tVendor\\Loader->boot\t1\t\t/app/vendor/autoload.php\t12
4\t3\t0\t0.0\t0\tReflectionClass->getName\t0\t\t/app/vendor/Loader.php\t20
4\t3\t1\t0.0\t0
3\t2\t1\t0.0\t0
2\t1\t1\t0.0\t0
1\t0\t1\t0.0\t0
TRACE END   [2026-01-01 00:00:00.000000]
"""


class NormalizeXdebugTraceTest(unittest.TestCase):
    """Verify caller reconstruction and semantic classification on a small trace."""

    def test_records_nested_edges_and_include_target(self) -> None:
        """Keep parent relationships and inclusion operands intact across trace exits."""

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            trace = root / "sample.xt"
            output = root / "ledger"
            trace.write_text(TRACE, encoding="utf-8")

            events = MODULE.parse_trace(trace)
            MODULE.write_ledgers(events, output)

            self.assertEqual(4, len(events))
            self.assertEqual("{main}", events[1].caller)
            self.assertEqual("include", events[1].kind)
            self.assertEqual("/app/vendor/autoload.php", events[1].include_target)
            self.assertEqual("Vendor\\Loader->boot", events[2].callee)
            self.assertEqual("reflection", events[3].kind)

            summary = json.loads((output / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(4, summary["entry_events"])
            self.assertEqual(1, summary["distinct_include_targets"])
            self.assertEqual({"function": 1, "include": 1, "reflection": 1, "instance_method": 1}, summary["events_by_kind"])


if __name__ == "__main__":
    unittest.main()
