"""Quota exhaustion preserves cleanup work; permissions and malformed data still fail."""

import json
import subprocess
import unittest
from unittest.mock import patch

from artifact_retention_api import RateDeferred, RetentionAPI, run_safely
from test_artifact_retention import FakeRetention, fixture, OLD, HISTORY


class QuotaTests(unittest.TestCase):
    def test_mutations_are_spaced_but_reads_do_not_sleep(self):
        api = RetentionAPI("o/r")
        with patch("artifact_retention_api.gh"), \
                patch("artifact_retention_api.time.monotonic", side_effect=[10, 10.2, 11.3]), \
                patch("artifact_retention_api.time.sleep") as sleep:
            api.request("actions/artifacts/1", "DELETE")
            api.request("actions/runs/2")
            api.request("actions/artifacts/3", "DELETE")
        self.assertEqual(sleep.call_count, 1)
        self.assertAlmostEqual(sleep.call_args.args[0], 0.8)

    def test_live_token_budget_keeps_a_reserve_and_defers_when_low(self):
        api = RetentionAPI("o/r")
        for remaining in (0, 200, 450, 15000):
            result = subprocess.CompletedProcess([], 0, json.dumps({
                "resources": {"core": {"remaining": remaining}}}))
            with patch("artifact_retention_api.subprocess.run", return_value=result):
                if remaining <= 200:
                    with self.assertRaises(RateDeferred):
                        api.budget()
                else:
                    self.assertEqual(api.budget(), remaining - 200)

    def test_rate_limited_deletion_keeps_pending_checkpoint_and_stops_continuation(self):
        janitor = FakeRetention([fixture(10, OLD), fixture(11)], True)
        error = subprocess.CalledProcessError(1, ["gh"], stderr="API rate limit exceeded (HTTP 403)")
        with patch.object(janitor, "remove", side_effect=error), \
                patch("artifact_retention.git", return_value="\n".join(HISTORY)):
            result = run_safely(janitor, 10)
        self.assertIn("rate limit", result["deferred"])
        self.assertFalse(result["continue_cleanup"])
        self.assertEqual(janitor.saved[-1]["pending"], [10, 11])
        self.assertEqual(janitor.saved[-1]["latest"]["run_id"], 11)

    def test_deletions_fit_the_live_quota_instead_of_the_fixed_batch_cap(self):
        janitor = FakeRetention([fixture(10, OLD), fixture(11)], True)
        with patch.object(janitor, "request_budget", return_value=2):
            result = janitor.execute()
        self.assertEqual(len(janitor.deleted), 2)
        self.assertTrue(result["continue_cleanup"])
        self.assertIn(10, janitor.saved[-1]["pending"])

    def test_low_quota_before_discovery_does_not_read_or_change_the_ledger(self):
        janitor = FakeRetention([fixture()], True)
        with patch.object(janitor, "request_budget", side_effect=RateDeferred("low quota")), \
                patch.object(janitor, "load") as load:
            result = run_safely(janitor, 10)
        self.assertTrue(result["deferred"])
        self.assertFalse(result["continue_cleanup"])
        load.assert_not_called()
        self.assertEqual((janitor.saved, janitor.deleted), ([], []))

    def test_secondary_limits_defer_but_permission_and_server_errors_propagate(self):
        janitor = FakeRetention([fixture()], True)
        for message, deferred in (("secondary rate limit (HTTP 429)", True),
                                  ("Resource not accessible (HTTP 403)", False),
                                  ("Server Error (HTTP 502)", False)):
            error = subprocess.CalledProcessError(1, ["gh"], stderr=message)
            with patch.object(janitor, "run", side_effect=error):
                if deferred:
                    self.assertTrue(run_safely(janitor)["deferred"])
                else:
                    with self.assertRaises(subprocess.CalledProcessError):
                        run_safely(janitor)

    def test_post_cleanup_quota_defers_both_continuation_and_package_refresh(self):
        janitor = FakeRetention([fixture()], True)
        completed = {"continue_cleanup": True, "refresh": [{"id": 1}], "latest_sha": "a" * 40}
        with patch.object(janitor, "run", return_value=completed), \
                patch.object(janitor, "request_budget", side_effect=RateDeferred("low quota")):
            result = run_safely(janitor)
        self.assertEqual(result["latest_sha"], "a" * 40)
        self.assertFalse(result["continue_cleanup"])
        self.assertEqual(result["refresh"], [])
        self.assertEqual(result["deferred"], "low quota")


if __name__ == "__main__":
    unittest.main()
