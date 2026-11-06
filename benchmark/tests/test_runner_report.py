"""Checks execution correctness, CLI filtering, deterministic reports, and dashboard escaping."""

import json
import subprocess
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path
from unittest.mock import patch

from support import record
from cases import discover
from common import ROOT, SUITES, atomic_json, measure, require_baselines
from dashboard import build_page
from history import empty_history, merge
from report import epoch, render_svg


class RunnerReportTests(unittest.TestCase):
    def test_all_fixture_paths_and_metadata_are_valid(self):
        suites = discover(list(SUITES), [])
        self.assertEqual(set(suites), set(SUITES))
        self.assertEqual(len(suites["native"]), 3)
        self.assertGreater(len(suites["magician"]), 5)
        self.assertEqual(len(suites["hot-path"]), 1)
        with self.assertRaises(ValueError):
            discover(["native"], ["does-not-exist"])

    def test_list_does_not_build_and_case_filter_is_precise(self):
        result = subprocess.run([sys.executable, "-B", str(ROOT / "run.py"), "--list", "--case", "native/sum_loop"],
                                capture_output=True, text=True, check=True)
        self.assertEqual(result.stdout, "native/sum_loop\n")

    def test_ci_requires_php_and_c_but_hot_path_has_no_external_baselines(self):
        suites = discover(list(SUITES), [])
        with patch("common.shutil.which", return_value=None):
            with self.assertRaisesRegex(ValueError, "PHP interpreter"):
                require_baselines(suites)
            require_baselines({"hot-path": suites["hot-path"]})
        with patch("common.shutil.which", side_effect=lambda name: "/usr/bin/php" if name == "php" else None):
            with self.assertRaisesRegex(ValueError, "C compiler"):
                require_baselines(suites)
        with patch("common.shutil.which", side_effect=lambda name: f"/usr/bin/{name}" if name in {"php", "clang"} else None):
            require_baselines(suites)

    def test_missing_required_baseline_fails_before_build_or_output(self):
        with tempfile.TemporaryDirectory(prefix="elephc-benchmark-test-") as directory:
            output = Path(directory) / "results.json"
            result = subprocess.run([sys.executable, "-B", str(ROOT / "run.py"), "--suite", "native",
                                     "--require-baselines", "--json", str(output)],
                                    env={"PATH": directory}, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("PHP interpreter", result.stderr)
            self.assertFalse(output.exists())

    def test_measure_validates_warmup_and_every_execution(self):
        with tempfile.TemporaryDirectory(prefix="elephc-benchmark-test-") as directory:
            cwd = Path(directory)
            result = measure([sys.executable, "-c", "print('correct')"], cwd, "correct\n", 3, 1, 10)
            self.assertEqual(len(result["samples_ms"]), 3)
            self.assertGreater(result["median_ms"], 0)
            with self.assertRaises(RuntimeError):
                measure([sys.executable, "-c", "print('wrong')"], cwd, "correct\n", 1, 1, 10)
            with self.assertRaises(subprocess.TimeoutExpired):
                measure([sys.executable, "-c", "import time; time.sleep(10)"], cwd, "", 1, 0, .05)

    def test_every_suite_renders_deterministic_valid_svg_in_both_themes(self):
        for suite in SUITES:
            history = merge(empty_history(suite), [record(suite)])
            for dark in (False, True):
                svg = render_svg(history, dark)
                self.assertEqual(svg, render_svg(history, dark))
                ET.fromstring(svg)
                self.assertIn("example", svg)
                self.assertNotIn("nan", svg.lower())
                self.assertNotIn("inf", svg.lower())
        ET.fromstring(render_svg(empty_history("hot-path")))

    def test_reports_skip_unavailable_baselines_and_split_changed_epochs(self):
        a = record()
        a["cases"][0]["variants"]["php"] = {"median_ms": None, "skipped": True, "samples_ms": []}
        a["cases"][0]["variants"]["c"] = {"median_ms": None, "skipped": True, "samples_ms": []}
        b = record(day="2026-10-11", run_id=2)
        b["cases"][0]["fixture_sha256"] = "c" * 64
        self.assertNotEqual(epoch(a, a["cases"][0]), epoch(b, b["cases"][0]))
        self.assertIn("PHP absent", render_svg(merge(empty_history("native"), [a])))
        svg = render_svg(merge(empty_history("native"), [a, b]))
        self.assertEqual(svg.count("<polyline"), 2)

    def test_dashboard_is_offline_and_cannot_break_out_of_embedded_data(self):
        with tempfile.TemporaryDirectory(prefix="elephc-benchmark-test-") as directory:
            root = Path(directory)
            for suite in SUITES:
                sample = record(suite)
                sample["cases"][0]["metadata"]["description"] = "</script><script>alert(1)</script>"
                atomic_json(root / f"{suite}.json", merge(empty_history(suite), [sample]))
            page = build_page(root).decode()
            self.assertNotIn("</script><script>alert", page)
            self.assertNotIn("cdn.", page)
            self.assertIn('"magician"', page)
            self.assertNotIn("__DATA__", page)


if __name__ == "__main__":
    unittest.main()
