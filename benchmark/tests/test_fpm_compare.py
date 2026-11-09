"""Unit tests for the Elephc vs Nginx+PHP-FPM comparison runner."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web"))

import fpm_compare


class FpmCompareTests(unittest.TestCase):
    def test_parse_origin_uses_default_port_and_path(self):
        host, port, target = fpm_compare.parse_origin("http://127.0.0.1/base?x=1")
        self.assertEqual(host, "127.0.0.1")
        self.assertEqual(port, 80)
        self.assertEqual(target, "/base?x=1")

    def test_summarize_reports_median_and_failures(self):
        summary = fpm_compare.summarize(
            [
                {"requests_per_sec": 100.0, "requests": 10, "non_2xx": 1},
                {"requests_per_sec": 300.0, "requests": 20, "non_2xx": 2},
            ]
        )
        self.assertEqual(summary["requests_per_sec"], 200.0)
        self.assertEqual(summary["requests"], 15.0)
        self.assertEqual(summary["failures"], 3)

    def test_compare_and_markdown_keep_both_sides(self):
        elephc = {
            fpm_compare.WRK_SCENARIO: {
                "requests_per_sec": 100.0,
                "latency_p50_ms": 1.0,
                "latency_p99_ms": 2.0,
            }
        }
        fpm = {
            fpm_compare.WRK_SCENARIO: {
                "requests_per_sec": 50.0,
                "latency_p50_ms": 2.0,
                "latency_p99_ms": 4.0,
            }
        }
        compared = fpm_compare.compare(elephc, fpm)
        self.assertEqual(compared[fpm_compare.WRK_SCENARIO]["throughput_ratio"], "0.50x")
        args = type(
            "Args",
            (),
            {"elephc_url": "http://elephc/", "fpm_url": "http://fpm/", "repeat": 1},
        )()
        report = fpm_compare.markdown_report(args, compared)
        self.assertIn("Elephc vs Nginx + PHP-FPM", report)
        self.assertIn("0.50x", report)
        self.assertIn("Throughput", report)


if __name__ == "__main__":
    unittest.main()
