"""Cover complete job inventories, bounded recovery and lifecycle caller integration."""

import subprocess
import unittest
from unittest.mock import Mock, patch
from urllib.parse import parse_qs, urlsplit

from artifact_retention import Retention
from github_jobs import read_jobs
from reconcile_development import Reconciler


class JobInventoryTests(unittest.TestCase):
    """Smaller pages may recover transport failures but must never weaken the verdict."""

    def test_successful_large_pages_need_no_retry(self):
        """Ordinary runs keep the existing efficient hundred-job pagination."""
        jobs = [{"id": number} for number in range(125)]
        api = Mock(side_effect=[{"jobs": jobs[:100], "total_count": 125},
                                {"jobs": jobs[100:], "total_count": 125}])
        self.assertEqual(read_jobs(api, "actions/runs/1/jobs?filter=latest"), jobs)
        self.assertEqual(api.call_count, 2)

    def test_later_large_page_failure_restarts_without_partial_results(self):
        """Restart page one at the new size, preserving the run and all-attempt filter."""
        jobs = [{"id": number} for number in range(125)]
        calls = []

        def api(path):
            """Model a partial rerun whose second large page consistently times out."""
            calls.append(path)
            query = parse_qs(urlsplit(path).query)
            self.assertEqual(query["filter"], ["all"])
            size, page = int(query["per_page"][0]), int(query["page"][0])
            if size == 100:
                if page == 2:
                    raise subprocess.CalledProcessError(1, ["gh"], stderr="HTTP 502")
                return {"jobs": [{"id": -1}] * 100, "total_count": 125}
            self.assertEqual(size, 20)
            return {"jobs": jobs[(page - 1) * size:page * size], "total_count": 125}

        self.assertEqual(read_jobs(api, "actions/runs/7/jobs?filter=all"), jobs)
        self.assertEqual(len(calls), 9)
        self.assertTrue(calls[2].endswith("per_page=20&page=1"))

    def test_only_transient_server_errors_allow_one_smaller_inventory(self):
        """Permission, rate-limit and missing-resource errors must propagate immediately."""
        for status in (403, 404, 429, 502, 503, 504):
            with self.subTest(status=status):
                error = subprocess.CalledProcessError(1, ["gh"], stderr=f"HTTP {status}")
                api = Mock(side_effect=error)
                with self.assertRaises(subprocess.CalledProcessError):
                    read_jobs(api, "actions/runs/1/jobs?filter=latest")
                self.assertEqual(api.call_count, 2 if status in (502, 503, 504) else 1)

    def test_incomplete_or_oversized_inventories_never_become_proof(self):
        """Reject short pages, invalid counts and inventory growth beyond the fixed cap."""
        for response in ({"jobs": [], "total_count": 1},
                         {"jobs": [{"id": 1}], "total_count": 0},
                         {"jobs": [], "total_count": 2001},
                         {"jobs": [], "total_count": -1},
                         {"jobs": [], "total_count": "0"},
                         {"jobs": {}, "total_count": 0}):
            with self.subTest(response=response):
                api = Mock(return_value=response)
                with self.assertRaises(ValueError):
                    read_jobs(api, "actions/runs/1/jobs?filter=latest")
                self.assertEqual(api.call_count, 1)

    def test_small_page_recovery_keeps_the_same_item_limit(self):
        """Reducing page size must not silently shrink or expand the accepted inventory."""
        jobs = [{"id": number} for number in range(40)]
        api = Mock(side_effect=[subprocess.CalledProcessError(1, ["gh"], stderr="HTTP 502"),
                                {"jobs": jobs[:20], "total_count": 40},
                                {"jobs": jobs[20:], "total_count": 40}])
        self.assertEqual(read_jobs(api, "actions/runs/1/jobs", max_items=40), jobs)
        self.assertEqual(api.call_count, 3)

    def test_retention_and_reconciliation_use_the_same_recovery(self):
        """The queue retry must not subsequently block cleanup or trigger extra backfills."""
        for worker, selection in ((Retention("o/r"), "latest"), (Reconciler("o/r"), "all")):
            with self.subTest(worker=type(worker).__name__):
                jobs = [{"id": 1, "name": "package", "conclusion": "success"}]
                with patch.object(worker, "api", side_effect=[
                        subprocess.CalledProcessError(1, ["gh"], stderr="HTTP 502"),
                        {"jobs": jobs, "total_count": 1}]) as api:
                    path = f"actions/runs/1/jobs?filter={selection}"
                    self.assertEqual(worker.pages(path, "jobs"), jobs)
                    self.assertEqual(api.call_args.args, (path + "&per_page=20&page=1",))


if __name__ == "__main__":
    unittest.main()
