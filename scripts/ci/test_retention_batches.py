"""Bounded cleanup must converge while preserving active readers and the latest download."""

import copy
import unittest
from unittest.mock import patch

from test_artifact_retention import FakeRetention, fixture, candidate, REPO, HISTORY, OLD


class BatchTests(unittest.TestCase):
    """Exercise real state transitions across successive janitor invocations."""

    def test_successive_batches_drain_backlog_and_keep_only_latest_complete_set(self):
        records = [fixture(10, OLD), fixture(11), fixture(12)]
        state = None
        deleted = set()
        with patch("artifact_retention.MAX_RUNS", 1), patch("artifact_retention.MAX_DELETIONS", 2):
            for attempt in range(12):
                remaining = [(run, jobs, [a for a in artifacts if a["id"] not in deleted])
                             for run, jobs, artifacts in records]
                janitor = FakeRetention(remaining, True)
                if state:
                    janitor.state = state
                result = janitor.execute()
                deleted.update(janitor.deleted)
                state = copy.deepcopy(janitor.saved[-1])
                self.assertLessEqual(len(janitor.deleted), 2)
                if not result["continue_cleanup"]:
                    break
            else:
                self.fail("cleanup never converged")
        self.assertEqual(state["latest"]["run_id"], 12)
        self.assertEqual(state["pending"], [])
        self.assertEqual({a["id"] for _, _, artifacts in records for a in artifacts} - deleted,
                         {1200, 1201, 1202})

    def test_active_only_backlog_does_not_self_dispatch_forever(self):
        latest = fixture(10)
        records = [fixture(11), fixture(12), latest]
        for run, _, _ in records[:2]:
            run["status"] = "in_progress"
        janitor = FakeRetention(records, True, candidate(*latest, REPO, HISTORY))
        # The latest package run has no remaining disposable inputs.
        janitor.records[10][2].pop()
        with patch("artifact_retention.MAX_RUNS", 1):
            result = janitor.execute()
        self.assertFalse(result["continue_cleanup"])
        self.assertEqual(janitor.saved[-1]["pending"], [12, 11])
        self.assertEqual(janitor.deleted, [])


if __name__ == "__main__":
    unittest.main()
