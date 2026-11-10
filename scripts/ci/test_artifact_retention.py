"""Exercise latest-only retention with failures, overlapping consumers and stale snapshots."""

import copy
import subprocess
import unittest
from unittest.mock import patch

from artifact_retention import Retention, iso
from artifact_retention_policy import (CI, DEVELOPMENT, RETENTION, candidate, keep_ids,
                                       newest, refresh_due, timestamp)
from resolve_main_build import GATES, HOSTS

REPO = "o/r"
SHA, OLD, FUTURE = "a" * 40, "b" * 40, "c" * 40
NOW = timestamp("2026-11-08T12:00:00Z")
HISTORY = [SHA, OLD]


def fixture(identifier=10, sha=SHA, path=CI):
    """Build a complete publication with disposable inputs alongside the three packages."""
    run = dict(id=identifier, head_sha=sha, head_branch="main", path=path,
               repository={"full_name": REPO}, event="push", status="completed",
               conclusion="success", run_attempt=1, updated_at=iso(NOW - 3600))
    jobs = [dict(name=gate, conclusion="success") for gate in GATES]
    artifacts = []
    for index, target in enumerate(HOSTS):
        jobs.append(dict(name=f"Package development build ({target})", conclusion="success"))
        artifacts.append(dict(id=identifier * 100 + index,
                              name=f"elephc-dev-{target}-{sha}-1", size_in_bytes=10,
                              expired=False, digest="sha256:" + "0" * 64,
                              created_at=iso(NOW), expires_at=iso(NOW + 90 * 86400),
                              workflow_run={"head_sha": sha}))
    artifacts.append(dict(id=identifier * 100 + 4, name="junit-all", size_in_bytes=100))
    return run, jobs, artifacts


def queue(identifier=20, sha=SHA):
    """A green queue needs only compiler/bridge pairs and the environment for main reuse."""
    run, jobs, artifacts = fixture(identifier, sha)
    run.update(event="merge_group", head_branch="gh-readonly-queue/main/pr-1-a")
    artifacts = artifacts[-1:]
    for target in HOSTS:
        for kind in ("compiler", "bridges"):
            artifacts.append(dict(id=identifier * 100 + len(artifacts) + 10,
                                  name=f"build-{target}-debug-{kind}-default-1",
                                  size_in_bytes=20, expired=False, workflow_run={"head_sha": sha}))
    artifacts.append(dict(id=identifier * 100 + 30, name="ci-environment-1", size_in_bytes=10))
    return run, jobs, artifacts


class RetentionPolicyTests(unittest.TestCase):
    """Never replace a usable complete set with incomplete or older work."""

    def test_only_complete_successful_main_packages_can_be_selected(self):
        run, jobs, artifacts = fixture()
        self.assertEqual(candidate(run, jobs, artifacts, REPO, HISTORY)["sha"], SHA)
        for field, value in (("conclusion", "failure"), ("status", "in_progress"),
                             ("event", "pull_request"), ("head_branch", "topic"),
                             ("path", ".github/workflows/other.yml"),
                             ("repository", {"full_name": "fork/r"}), ("head_sha", FUTURE)):
            with self.subTest(field=field):
                self.assertIsNone(candidate(dict(run, **{field: value}), jobs, artifacts, REPO, HISTORY))
        for index in range(3):
            self.assertIsNone(candidate(run, jobs, artifacts[:index] + artifacts[index+1:], REPO, HISTORY))
        for index in range(len(jobs)):
            bad = copy.deepcopy(jobs)
            bad[index]["conclusion"] = "skipped"
            self.assertIsNone(candidate(run, bad, artifacts, REPO, HISTORY))
        for field, value in (("expired", True), ("size_in_bytes", 0),
                             ("workflow_run", {"head_sha": OLD})):
            bad = copy.deepcopy(artifacts)
            bad[0][field] = value
            self.assertIsNone(candidate(run, jobs, bad, REPO, HISTORY))

    def test_late_old_build_cannot_replace_newer_main_source(self):
        current = candidate(*fixture(), REPO, HISTORY)
        older = candidate(*fixture(99, OLD), REPO, HISTORY)
        self.assertEqual(newest(current, [older], HISTORY), current)
        self.assertEqual(newest(older, [current], HISTORY), current)
        with self.assertRaisesRegex(ValueError, "left main"):
            newest(current, [], [OLD])

    def test_backfill_source_must_match_its_validated_dispatch_title(self):
        run, jobs, artifacts = fixture(path=DEVELOPMENT)
        run.update(event="workflow_dispatch", display_title=f"Development artifacts {SHA}")
        self.assertIsNotNone(candidate(run, jobs, artifacts, REPO, HISTORY))
        run["display_title"] = f"Development artifacts {OLD}"
        self.assertIsNone(candidate(run, jobs, artifacts, REPO, HISTORY))

    def test_refresh_replaces_only_after_all_three_copy_jobs_pass(self):
        previous = candidate(*fixture(), REPO, HISTORY)
        run, jobs, artifacts = fixture(11, path=RETENTION)
        run["event"] = "schedule"
        for job in jobs:
            job["name"] = job["name"].replace("Package development", "Refresh development")
        for artifact in artifacts[:3]:
            artifact["expires_at"] = iso(NOW + 170 * 86400)
        refreshed = candidate(run, jobs, artifacts, REPO, HISTORY)
        self.assertEqual(newest(previous, [refreshed], HISTORY), refreshed)
        jobs[-1]["conclusion"] = "failure"
        self.assertIsNone(candidate(run, jobs, artifacts, REPO, HISTORY))

    def test_active_foreign_and_newer_than_checkout_runs_are_untouched(self):
        for changes in ({"status": "in_progress"}, {"head_sha": FUTURE},
                        {"repository": {"full_name": "fork/r"}}):
            run, jobs, artifacts = fixture()
            run.update(changes)
            self.assertEqual(keep_ids(run, jobs, artifacts, None, REPO, HISTORY, set(), False, NOW),
                             {a["id"] for a in artifacts})

    def test_latest_main_keeps_only_three_packages_and_failed_pr_keeps_nothing(self):
        run, jobs, artifacts = fixture()
        latest = candidate(run, jobs, artifacts, REPO, HISTORY)
        self.assertEqual(keep_ids(run, jobs, artifacts, latest, REPO, HISTORY, set(), False, NOW),
                         {a["id"] for a in artifacts[:3]})
        run.update(event="pull_request", head_branch="feature", conclusion="failure")
        self.assertEqual(keep_ids(run, jobs, artifacts, None, REPO, HISTORY, set(), False, NOW), set())

    def test_queue_drops_test_payloads_but_keeps_seven_handoff_artifacts(self):
        run, jobs, artifacts = queue()
        keep = keep_ids(run, jobs, artifacts, None, REPO, HISTORY, set(), True, NOW)
        self.assertEqual(keep, {a["id"] for a in artifacts[1:]})
        latest = candidate(*fixture(), REPO, HISTORY)
        self.assertEqual(keep_ids(run, jobs, artifacts, latest, REPO, HISTORY, set(), False, NOW), set())
        self.assertEqual(keep_ids(run, jobs, artifacts, latest, REPO, HISTORY, {SHA}, False, NOW), keep)
        jobs[0]["conclusion"] = "failure"
        self.assertEqual(keep_ids(run, jobs, artifacts, None, REPO, HISTORY, set(), True, NOW), set())

    def test_abandoned_queue_releases_after_merge_propagation_grace(self):
        run, jobs, artifacts = queue(sha=FUTURE)
        self.assertEqual(keep_ids(run, jobs, artifacts, None, REPO, HISTORY, set(), False, NOW), set())
        run["updated_at"] = iso(NOW - 30)
        self.assertEqual(len(keep_ids(run, jobs, artifacts, None, REPO, HISTORY, set(), False, NOW)), 7)

    def test_late_optional_queue_failure_does_not_remove_active_main_inputs(self):
        run, jobs, artifacts = queue()
        for conclusion in ("failure", "cancelled"):
            run["conclusion"] = conclusion
            self.assertEqual(len(keep_ids(run, jobs, artifacts, None, REPO, HISTORY, {SHA}, False, NOW)), 7)
            self.assertEqual(keep_ids(run, jobs, artifacts, None, REPO, HISTORY, set(), False, NOW), set())

    def test_refresh_starts_before_expiry_even_with_shorter_organization_cap(self):
        latest = candidate(*fixture(), REPO, HISTORY)
        self.assertFalse(refresh_due(latest, NOW))
        self.assertTrue(refresh_due(latest, NOW + 82 * 86400))
        for artifact in latest["artifacts"].values():
            artifact["expires_at"] = iso(NOW + 86400)
        self.assertTrue(refresh_due(latest, NOW + 20 * 3600))


class FakeRetention(Retention):
    """Model API snapshots and record writes at the transport boundary."""

    def __init__(self, records, apply=False, latest=None):
        super().__init__(REPO, apply, NOW)
        self.records = {run["id"]: (run, jobs, artifacts) for run, jobs, artifacts in records}
        self.state = dict(schema=1, since=iso(NOW), pending=list(self.records), queues=[], latest=latest)
        self.saved, self.deleted, self.active, self.reread = [], [], [], {}

    def load(self):
        return copy.deepcopy(self.state)

    def request_budget(self):
        return 10000

    def save(self, state):
        if self.apply:
            self.saved.append(copy.deepcopy(state))

    def active_runs(self):
        return {run["id"]: run for run in self.active}

    def queue_exists(self, run):
        return True

    def api(self, endpoint, method="GET", body=None):
        if method == "DELETE":
            self.deleted.append(int(endpoint.rsplit("/", 1)[1]))
            return None
        prefix = endpoint.split("?", 1)[0]
        parts = prefix.split("/")
        identifier = int(parts[2])
        run, jobs, artifacts = self.records[identifier]
        if len(parts) == 3:
            changed = self.reread.get(identifier, [])
            return copy.deepcopy(changed.pop(0) if changed else run)
        items = jobs if parts[3] == "jobs" else artifacts
        return {parts[3]: copy.deepcopy(items), "total_count": len(items)}

    def execute(self):
        with patch("artifact_retention.git", return_value="\n".join(HISTORY)):
            return self.run(run_id=next(iter(self.records)))


class RetentionDriverTests(unittest.TestCase):
    """API failures and interruptions must leave the last complete download protected."""

    def test_dry_run_never_deletes_or_persists_and_apply_checkpoints_before_delete(self):
        records = [fixture(10, OLD), fixture(11)]
        old = candidate(*records[0], REPO, HISTORY)
        dry = FakeRetention(records, latest=old)
        self.assertEqual(sum(r["delete_count"] for r in dry.execute()["runs"]), 5)
        self.assertEqual((dry.saved, dry.deleted), ([], []))
        real = FakeRetention(records, apply=True, latest=old)
        remove = real.remove
        def checkpointed(artifact):
            self.assertEqual(real.saved[-1]["latest"]["run_id"], 11)
            self.assertIn(10, real.saved[-1]["pending"])
            remove(artifact)
        with patch.object(real, "remove", side_effect=checkpointed):
            real.execute()
        self.assertEqual(set(real.deleted), {1000, 1001, 1002, 1004, 1104})

    def test_benchmark_archive_failure_prevents_deleting_its_source(self):
        """A failed durable write must leave the original measurement downloadable."""
        run, jobs, artifacts = fixture()
        artifacts.append(dict(id=1005, name="benchmark-results", size_in_bytes=200))
        janitor = FakeRetention([(run, jobs, artifacts)], True)
        with patch.object(janitor, "preserve_benchmarks", create=True,
                          side_effect=RuntimeError("archive unavailable")):
            with self.assertRaisesRegex(RuntimeError, "archive unavailable"):
                janitor.execute()
        self.assertNotIn(1005, janitor.deleted)

    def test_cleanup_batches_checkpoint_unprocessed_runs_and_partial_deletions(self):
        """Large backlogs must make bounded progress without forgetting remaining inputs."""
        records = [fixture(10, OLD), fixture(11), fixture(12)]
        janitor = FakeRetention(records, True)
        with patch("artifact_retention.MAX_RUNS", 2, create=True), \
                patch("artifact_retention.MAX_DELETIONS", 2, create=True):
            result = janitor.execute()
        self.assertEqual(len(janitor.deleted), 2)
        self.assertTrue(result["continue_cleanup"])
        self.assertIn(10, janitor.saved[-1]["pending"])
        self.assertIn(12, janitor.saved[-1]["pending"])

    def test_completed_pr_cleanup_needs_no_job_inventory(self):
        """PR jobs cannot select main packages or a queue handoff."""
        run, jobs, artifacts = fixture()
        run.update(event="pull_request", head_branch="feature")
        janitor = FakeRetention([(run, jobs, artifacts)], True)
        request = janitor.api
        def no_jobs(endpoint, method="GET", body=None):
            self.assertNotIn("/jobs", endpoint)
            return request(endpoint, method, body)
        with patch.object(janitor, "api", side_effect=no_jobs):
            janitor.execute()
        self.assertEqual(len(janitor.deleted), len(artifacts))

    def test_partial_new_set_preserves_previous_download(self):
        old = fixture(10, OLD)
        run, jobs, artifacts = fixture(11)
        artifacts.pop(0)
        janitor = FakeRetention([old, (run, jobs, artifacts)], True, candidate(*old, REPO, HISTORY))
        janitor.execute()
        self.assertFalse({1000, 1001, 1002} & set(janitor.deleted))

    def test_interruption_after_selection_preserves_cleanup_work_for_retry(self):
        records = [fixture(10, OLD), fixture(11)]
        old = candidate(*records[0], REPO, HISTORY)
        interrupted = FakeRetention(records, True, old)
        with patch.object(interrupted, "remove", side_effect=RuntimeError("runner stopped")):
            with self.assertRaisesRegex(RuntimeError, "runner stopped"):
                interrupted.execute()
        checkpoint = interrupted.saved[-1]
        self.assertEqual(checkpoint["latest"]["run_id"], 11)
        self.assertEqual(checkpoint["pending"], [10, 11])
        retry = FakeRetention(records, True)
        retry.state = checkpoint
        retry.execute()
        self.assertEqual(set(retry.deleted), {1000, 1001, 1002, 1004, 1104})

    def test_real_dry_run_bootstrap_never_writes_to_the_api(self):
        janitor = Retention(REPO, False, NOW)
        absent = subprocess.CalledProcessError(1, ["gh"], stderr="HTTP 404")
        with patch.object(janitor, "api", side_effect=absent) as api, \
                patch("artifact_retention.git", return_value=""):
            state = janitor.load()
            janitor.save(state)
            janitor.remove({"id": 1})
        self.assertEqual(len(api.call_args_list), 1)
        self.assertEqual(api.call_args.args, ("contents/artifact-retention.json?ref=ci/development-artifact-ledger",))

    def test_missing_old_run_is_forgotten_but_missing_latest_run_blocks_cleanup(self):
        record = fixture()
        janitor = FakeRetention([record], True)
        janitor.state["pending"].append(99)
        api = janitor.api
        def gone(endpoint, method="GET", body=None):
            if endpoint == "actions/runs/99":
                raise subprocess.CalledProcessError(1, ["gh"], stderr="HTTP 404")
            return api(endpoint, method, body)
        with patch.object(janitor, "api", side_effect=gone):
            janitor.execute()
        self.assertEqual(janitor.saved[-1]["pending"], [])
        latest = candidate(*record, REPO, HISTORY)
        latest["run_id"] = 99
        janitor = FakeRetention([record], True, latest)
        with patch.object(janitor, "api", side_effect=subprocess.CalledProcessError(
                1, ["gh"], stderr="HTTP 404")):
            with self.assertRaises(subprocess.CalledProcessError):
                janitor.execute()
        self.assertEqual(janitor.deleted, [])

    def test_queue_handoff_is_protected_for_active_main_and_backfill_readers(self):
        for path, event in ((CI, "push"), (DEVELOPMENT, "workflow_dispatch")):
            janitor = FakeRetention([queue(20), fixture(10)], True)
            reader, _, _ = fixture(30, path=path)
            reader.update(status="queued", event=event, display_title=f"Development artifacts {SHA}")
            janitor.active = [reader]
            janitor.execute()
            self.assertEqual(set(janitor.deleted), {2004, 1004})

    def test_new_rerun_and_run_finishing_during_inventory_are_revisited(self):
        record = fixture()
        for first, second in ((record[0], dict(record[0], run_attempt=2)),
                              (dict(record[0], status="in_progress"), record[0])):
            janitor = FakeRetention([record], True)
            janitor.reread[10] = [first, second]
            janitor.execute()
            self.assertEqual(janitor.deleted, [])
            self.assertEqual(janitor.saved[-1]["pending"], [10])

    def test_missing_retained_download_aborts_before_any_delete(self):
        run, jobs, artifacts = fixture()
        latest = candidate(run, jobs, artifacts, REPO, HISTORY)
        janitor = FakeRetention([(run, jobs, artifacts[1:])], True, latest)
        with self.assertRaisesRegex(ValueError, "missing or expired"):
            janitor.execute()
        self.assertEqual(janitor.deleted, [])

    def test_api_errors_abort_and_confirmed_missing_delete_is_idempotent(self):
        janitor = Retention(REPO, True, NOW)
        for code in (404, 403, 500):
            error = subprocess.CalledProcessError(1, ["gh"], stderr=f"HTTP {code}")
            with patch.object(janitor, "api", side_effect=error):
                if code == 404:
                    janitor.remove({"id": 1})
                else:
                    with self.assertRaises(subprocess.CalledProcessError):
                        janitor.remove({"id": 1})
        with patch.object(janitor, "api", return_value={"workflow_runs": [], "total_count": 1001}):
            with self.assertRaisesRegex(ValueError, "search limit"):
                janitor.pages("actions/runs", "workflow_runs")


if __name__ == "__main__":
    unittest.main()
