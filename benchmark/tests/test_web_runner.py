"""Unit tests for the standalone `benchmark/web` HTTP runner."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web"))

import run


WRK_SAMPLE = """Running 3s test @ http://127.0.0.1:8080/
  4 threads and 100 connections
  Thread Stats   Avg      Stdev     Max   +/- Stdev
    Latency   769.01us   100.00us   1.00ms   80.00%
    Req/Sec     1.00k     0.00     1.00k    100.00%
  Latency Distribution
     50%  700.00us
     75%  800.00us
     90%  900.00us
     99%    1.00ms
  1000 requests in 3.00s, 1.00MB read
Requests/sec:    333.33
Transfer/sec:    341.00KB
"""


class WebRunnerTests(unittest.TestCase):
    def test_parse_ms_handles_every_wrk_unit(self):
        self.assertEqual(run.parse_ms("769.01us"), 0.76901)
        self.assertEqual(run.parse_ms("12.5ms"), 12.5)
        self.assertEqual(run.parse_ms("1.5s"), 1500.0)

    def test_parse_wrk_extracts_throughput_and_latency(self):
        parsed = run.parse_wrk(WRK_SAMPLE)
        self.assertEqual(parsed["requests_per_sec"], 333.33)
        self.assertEqual(parsed["latency_avg_ms"], 0.76901)
        self.assertEqual(parsed["latency_p99_ms"], 1.0)
        self.assertEqual(parsed["transfer_per_sec"], "341.00KB")

    def test_aggregate_sums_throughput_across_clients(self):
        first = run.parse_wrk(WRK_SAMPLE)
        second = dict(first)
        second["requests_per_sec"] = 666.67
        second["latency_avg_ms"] = 2.0
        second["latency_p99_ms"] = 3.0
        merged = run.aggregate_wrk_results([first, second])
        self.assertEqual(merged["clients"], 2)
        self.assertAlmostEqual(merged["requests_per_sec"], 1000.0)
        self.assertAlmostEqual(merged["latency_avg_ms"], (0.76901 + 2.0) / 2)
        self.assertEqual(merged["latency_p99_ms"], 3.0)

    def test_summarize_reports_median_and_spread(self):
        summary = run.summarize(
            [
                {"requests_per_sec": 100.0, "latency_avg_ms": 1.0, "latency_p99_ms": 2.0},
                {"requests_per_sec": 300.0, "latency_avg_ms": 3.0, "latency_p99_ms": 4.0},
            ]
        )
        self.assertEqual(summary["requests_per_sec_median"], 200.0)
        self.assertEqual(summary["requests_per_sec_min"], 100.0)
        self.assertEqual(summary["requests_per_sec_max"], 300.0)
        self.assertEqual(summary["latency_avg_ms_median"], 2.0)

    def test_markdown_tolerates_a_missing_percentile(self):
        results = {
            "fixture": "hello.php",
            "workers": 4,
            "threads": 4,
            "connections": 100,
            "duration": "3s",
            "environment": {"platform": "test", "cpu_count": 8},
            "wrk": {
                "raw_rounds": [{"requests_per_sec": 100.0, "latency_avg_ms": 1.0}],
                "summary": run.summarize(
                    [{"requests_per_sec": 100.0, "latency_avg_ms": 1.0}]
                ),
            },
        }
        report = run.markdown_report(results)
        self.assertIn("n/a", report)
        self.assertIn("100 req/s", report)


if __name__ == "__main__":
    unittest.main()
