"""Importer regressions for daily sampling after reading platform and suite data."""

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from support import record
from common import SUITES, atomic_json
from fetch import refresh


def artifact(sample):
    """Build GitHub metadata and a complete shared artifact for a test record."""
    meta = {"id": sample["artifact_id"], "created_at": sample["timestamp"],
            "sha": sample["commit"], "run_id": sample["run_id"]}
    payload = {"schema_version": 2, "timestamp": sample["timestamp"],
               "commit": sample["commit"], "branch": "main", "run_id": sample["run_id"],
               "run_attempt": sample["run_attempt"], "environment": sample["environment"],
               "suites": {suite: {"cases": record(suite)["cases"]} for suite in SUITES}}
    return meta, payload


class FetchTests(unittest.TestCase):
    def setUp(self):
        """Existing artifact tests have no durable ledger unless explicitly supplied."""
        patcher = patch("fetch.list_archives", return_value=[])
        self.archives = patcher.start()
        self.addCleanup(patcher.stop)

    def test_daily_refresh_keeps_each_platform_and_caches_superseded_artifacts(self):
        first = record(hour="10")
        linux = record(hour="12", run_id=2, runner="ubuntu-24.04")
        linux["environment"].update({"os": "Linux", "architecture": "x86_64"})
        latest = record(hour="20", run_id=3)
        artifacts = [artifact(sample) for sample in (first, linux, latest)]
        payloads = {meta["id"]: payload for meta, payload in artifacts}
        with tempfile.TemporaryDirectory(prefix="elephc-benchmark-test-") as temporary:
            root = Path(temporary)

            def download(repo, meta, destination):
                atomic_json(destination / "results.json", payloads[meta["id"]])

            with patch("fetch.list_artifacts", return_value=[meta for meta, _ in artifacts]), \
                    patch("fetch.download_artifact", side_effect=download) as downloader:
                histories = refresh("owner/repo", "main", root)
                for history in histories.values():
                    self.assertEqual([r["run_id"] for r in history["series"]], [2, 3])
                    self.assertEqual([r["environment"]["runner"] for r in history["series"]],
                                     ["ubuntu-24.04", "macos-15"])
                self.assertEqual(downloader.call_count, 3)
                before = {path.name: path.read_bytes() for path in root.iterdir()}
                refresh("owner/repo", "main", root)
                self.assertEqual(downloader.call_count, 3)
                self.assertEqual(before, {path.name: path.read_bytes() for path in root.iterdir()})

                # A newer sample on another platform cannot stand in for missing data.
                native = histories["native"]
                native["series"] = [r for r in native["series"] if r["run_id"] == 3]
                native["points"] = 1
                atomic_json(root / "native.json", native)
                restored = refresh("owner/repo", "main", root)
                self.assertEqual(downloader.call_count, 4)
                self.assertEqual(downloader.call_args.args[1]["id"], 2)
                self.assertEqual([r["run_id"] for r in restored["native"]["series"]], [2, 3])

    def test_deleted_actions_results_are_recovered_from_durable_blobs_and_cached(self):
        """Deletion before the nightly importer must preserve daily and all-run histories."""
        samples = [record(hour="10"), record(hour="20", run_id=2),
                   record(day="2026-10-11", run_id=3)]
        self.archives.return_value = [{"id": s["artifact_id"], "sha": str(s["run_id"])} for s in samples]
        values = {s["artifact_id"]: (artifact(s)[0], {"native": s}) for s in samples}
        for all_runs in (False, True):
            with tempfile.TemporaryDirectory(prefix="elephc-benchmark-test-") as temporary, \
                    patch("fetch.list_artifacts", return_value=[]), \
                    patch("fetch.download_artifact") as download, \
                    patch("fetch.read_archive", side_effect=lambda repo, entry: values[entry["id"]]) as read:
                directory = Path(temporary)
                histories = refresh("owner/repo", "main", directory, all_runs)
                self.assertEqual([r["run_id"] for r in histories["native"]["series"]],
                                 [1, 2, 3] if all_runs else [2, 3])
                before = {p.name: p.read_bytes() for p in directory.iterdir()}
                refresh("owner/repo", "main", directory, all_runs)
                self.assertEqual(read.call_count, 3)
                self.assertEqual(before, {p.name: p.read_bytes() for p in directory.iterdir()})
                download.assert_not_called()

    def test_invalid_archive_leaves_existing_history_and_checkpoints_unchanged(self):
        """A later corrupt input must not partially publish earlier valid inputs."""
        self.archives.return_value = [{"id": 1, "sha": "one"}, {"id": 2, "sha": "two"}]
        sample = record()
        with tempfile.TemporaryDirectory(prefix="elephc-benchmark-test-") as temporary, \
                patch("fetch.list_artifacts", return_value=[]), \
                patch("fetch.read_archive", side_effect=[
                    (artifact(sample)[0], {"native": sample}), ValueError("bad archive")]):
            directory = Path(temporary)
            atomic_json(directory / "imports.json", {})
            before = {p.name: p.read_bytes() for p in directory.iterdir()}
            with self.assertRaisesRegex(ValueError, "bad archive"):
                refresh("owner/repo", "main", directory)
            self.assertEqual(before, {p.name: p.read_bytes() for p in directory.iterdir()})

    def test_archived_and_live_samples_are_selected_together_before_php_backfill(self):
        """An earlier archived sample must not supply a stale reference over a newer live sample."""
        missing = record(day="2026-10-09")
        missing["cases"][0]["variants"]["php"] = {"median_ms": None, "samples_ms": [], "skipped": True}
        early = record(hour="10", run_id=2)
        latest = record(hour="20", run_id=3)
        self.archives.return_value = [{"id": 1, "sha": "one"}, {"id": 2, "sha": "two"}]
        live_meta, payload = artifact(latest)
        payload["suites"]["native"]["cases"][0]["variants"]["php"] = {
            "median_ms": 10.0, "samples_ms": [9.0, 10.0, 11.0], "skipped": False}
        with tempfile.TemporaryDirectory(prefix="elephc-benchmark-test-") as temporary, \
                patch("fetch.list_artifacts", return_value=[live_meta]), \
                patch("fetch.read_archive", side_effect=[
                    (artifact(missing)[0], {"native": missing}), (artifact(early)[0], {"native": early})]), \
                patch("fetch.download_artifact", side_effect=lambda repo, meta, dest:
                      atomic_json(dest / "results.json", payload)):
            histories = refresh("owner/repo", "main", Path(temporary))
        reference = histories["native"]["series"][0]["cases"][0]["variants"]["php"]["reference"]
        self.assertEqual((reference["run_id"], reference["median_ms"]), (3, 10.0))

    def test_daily_refresh_selects_each_suite_from_partial_legacy_archives(self):
        artifacts = [artifact(record(hour="10"))[0], artifact(record(hour="20", run_id=2))[0]]
        filenames = {"native": "benchmark-results.json", "magician": "magician-benchmark-results.json"}
        with tempfile.TemporaryDirectory(prefix="elephc-benchmark-test-") as temporary:
            root = Path(temporary)

            def download(repo, meta, destination):
                suites = ("native", "magician") if meta["id"] == 1 else ("native",)
                for suite in suites:
                    case = record(suite)["cases"][0]
                    legacy = {"case": case["case"], "metadata": case["metadata"],
                              **{label.replace("-", "_"): value for label, value in case["variants"].items()}}
                    atomic_json(destination / filenames[suite],
                                {"iterations": 3, "warmup": 1, "cases": [legacy]})

            with patch("fetch.list_artifacts", return_value=artifacts), \
                    patch("fetch.download_artifact", side_effect=download) as downloader:
                histories = refresh("owner/repo", "main", root)
                self.assertEqual([r["run_id"] for r in histories["native"]["series"]], [2])
                self.assertEqual([r["run_id"] for r in histories["magician"]["series"]], [1])
                self.assertEqual(histories["hot-path"]["points"], 0)
                refresh("owner/repo", "main", root)
                self.assertEqual(downloader.call_count, 2)

    def test_php_backfill_uses_the_selected_daily_record(self):
        missing = record(day="2026-10-09")
        missing["cases"][0]["variants"]["php"] = {"median_ms": None, "samples_ms": [], "skipped": True}
        early = record(hour="10", run_id=2)
        latest = record(hour="20", run_id=3)
        artifacts = [artifact(sample) for sample in (missing, early, latest)]
        artifacts[0][1]["suites"]["native"]["cases"] = missing["cases"]
        artifacts[2][1]["suites"]["native"]["cases"][0]["variants"]["php"] = {
            "median_ms": 10.0, "samples_ms": [9.0, 10.0, 11.0], "skipped": False}
        payloads = {meta["id"]: payload for meta, payload in artifacts}
        with tempfile.TemporaryDirectory(prefix="elephc-benchmark-test-") as temporary:
            def download(repo, meta, destination):
                atomic_json(destination / "results.json", payloads[meta["id"]])

            with patch("fetch.list_artifacts", return_value=[meta for meta, _ in artifacts]), \
                    patch("fetch.download_artifact", side_effect=download):
                histories = refresh("owner/repo", "main", Path(temporary))
            reference = histories["native"]["series"][0]["cases"][0]["variants"]["php"]["reference"]
            self.assertEqual(reference["run_id"], 3)
            self.assertEqual(reference["median_ms"], 10.0)

    def test_all_runs_cache_recovers_an_earlier_missing_sample(self):
        artifacts = [artifact(record(hour="10")), artifact(record(hour="20", run_id=2))]
        payloads = {meta["id"]: payload for meta, payload in artifacts}
        with tempfile.TemporaryDirectory(prefix="elephc-benchmark-test-") as temporary:
            root = Path(temporary)

            def download(repo, meta, destination):
                atomic_json(destination / "results.json", payloads[meta["id"]])

            with patch("fetch.list_artifacts", return_value=[meta for meta, _ in artifacts]), \
                    patch("fetch.download_artifact", side_effect=download) as downloader:
                histories = refresh("owner/repo", "main", root, all_runs=True)
                self.assertTrue(all(h["points"] == 2 for h in histories.values()))
                refresh("owner/repo", "main", root, all_runs=True)
                self.assertEqual(downloader.call_count, 2)
                native = histories["native"]
                native["series"] = native["series"][1:]
                native["points"] = 1
                atomic_json(root / "native.json", native)
                restored = refresh("owner/repo", "main", root, all_runs=True)
                self.assertEqual(downloader.call_count, 3)
                self.assertEqual(downloader.call_args.args[1]["id"], 1)
                self.assertEqual(restored["native"]["points"], 2)


if __name__ == "__main__":
    unittest.main()
