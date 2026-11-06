"""Regressions for preserving expired history and importing both artifact formats."""

import copy
import json
import tempfile
import unittest
import zipfile
from subprocess import CompletedProcess
from pathlib import Path
from unittest.mock import patch

from support import record
from common import SUITES, atomic_json, effective_ms
from fetch import cached_import, download_artifact, list_artifacts, read_artifact, refresh
from history import current_results, empty_history, legacy_result, load_history, merge, migrate_native, validate_cases


class HistoryTests(unittest.TestCase):
    def test_backfill_covers_native_and_both_magician_php_variants(self):
        for suite in ("native", "magician"):
            old = record(suite, day="2026-10-09")
            first = record(suite, run_id=2)
            for label, variant in old["cases"][0]["variants"].items():
                if label.startswith("php"):
                    variant.update({"median_ms": None, "samples_ms": [], "skipped": True})
            history = merge(empty_history(suite), [old, first])
            for label, variant in history["series"][0]["cases"][0]["variants"].items():
                self.assertEqual(effective_ms(variant), 2.0)
                self.assertEqual("reference" in variant, label.startswith("php"))

    def test_php_backfill_preserves_raw_gaps_and_each_new_run_has_fresh_measurements(self):
        old = record(day="2026-10-09")
        old["cases"][0]["variants"]["php"] = {"median_ms": None, "samples_ms": [], "skipped": True}
        first = record(run_id=2)
        first["environment"].update({"php": "PHP 8.5.10", "php_mode": "CLI; OPcache/JIT disabled"})
        first["cases"][0]["variants"]["php"] = {"median_ms": 10.0, "samples_ms": [9.0, 10.0, 11.0], "skipped": False}
        later = copy.deepcopy(first)
        later.update({"date": "2026-10-11", "timestamp": "2026-10-11T12:00:00Z", "run_id": 3, "artifact_id": 3})
        later["cases"][0]["variants"]["php"] = {"median_ms": 20.0, "samples_ms": [19.0, 20.0, 21.0], "skipped": False}
        history = merge(empty_history("native"), [old, first, later])
        variants = [r["cases"][0]["variants"]["php"] for r in history["series"]]
        self.assertIsNone(variants[0]["median_ms"])
        self.assertEqual(variants[0]["samples_ms"], [])
        self.assertEqual([effective_ms(v) for v in variants], [10.0, 10.0, 20.0])
        self.assertEqual(variants[0]["reference"]["run_id"], 2)
        self.assertEqual(variants[0]["reference"]["environment"]["php"], "PHP 8.5.10")
        self.assertNotIn("reference", old["cases"][0]["variants"]["php"])
        self.assertNotIn("reference", variants[-1])
        self.assertEqual(merge(history, [later]), history)
        with tempfile.TemporaryDirectory(prefix="elephc-benchmark-test-") as temporary:
            path = Path(temporary) / "native.json"
            atomic_json(path, history)
            self.assertEqual(load_history(path, "native"), history)

    def test_backfill_rejects_known_mismatches_and_accepts_unknown_legacy_metadata(self):
        first = record(run_id=2)
        first["environment"].update({"php": "PHP 8.5.10", "php_mode": "CLI; OPcache/JIT disabled"})
        missing = record(day="2026-10-09")
        missing["cases"][0]["variants"]["php"] = {"median_ms": None, "samples_ms": [], "skipped": True}
        for field, value in {"runner": "linux", "os": "Linux", "architecture": "x86_64",
                             "php": "PHP 8.4.0", "php_mode": "JIT"}.items():
            old = copy.deepcopy(missing)
            old["environment"][field] = value
            history = merge(empty_history("native"), [old, first])
            self.assertNotIn("reference", history["series"][0]["cases"][0]["variants"]["php"])
        changed_fixture = copy.deepcopy(missing)
        changed_fixture["cases"][0]["fixture_sha256"] = "c" * 64
        history = merge(empty_history("native"), [changed_fixture, first])
        self.assertNotIn("reference", history["series"][0]["cases"][0]["variants"]["php"])
        legacy = copy.deepcopy(missing)
        legacy["environment"].update({"legacy": True, "runner": "unknown", "os": "unknown", "architecture": "unknown"})
        legacy["cases"][0]["fixture_sha256"] = None
        history = merge(empty_history("native"), [legacy, first])
        self.assertEqual(effective_ms(history["series"][0]["cases"][0]["variants"]["php"]), 2.0)

    def test_download_uses_exact_artifact_id_and_only_extracts_result_files(self):
        with tempfile.TemporaryDirectory(prefix="elephc-benchmark-test-") as temporary:
            directory = Path(temporary) / "selected"

            def request(command, *, stdout, **kwargs):
                self.assertEqual(command, ["gh", "api", "repos/owner/repo/actions/artifacts/42/zip"])
                with zipfile.ZipFile(stdout, "w") as archive:
                    archive.writestr("results.json", '{"source": "selected"}')
                    archive.writestr("../../unexpected.txt", "ignored")
                return CompletedProcess(command, 0, stderr="")

            with patch("fetch.subprocess.run", side_effect=request):
                download_artifact("owner/repo", {"id": 42, "run_id": 1}, directory)
            self.assertEqual([p.name for p in directory.iterdir()], ["results.json"])
            self.assertEqual(json.loads((directory / "results.json").read_text()), {"source": "selected"})

    def test_expired_history_and_new_days_survive(self):
        old = record(day="2026-04-01")
        new = record(day="2026-10-10", run_id=2)
        history = merge(merge(empty_history("native"), [old]), [new])
        self.assertEqual([r["date"] for r in history["series"]], ["2026-04-01", "2026-10-10"])
        self.assertEqual(merge(history, [new, old]), history)

    def test_newer_same_day_replaces_and_preserves_unknown_fields(self):
        old = record(hour="10")
        old["historical_note"] = "keep this"
        new = record(hour="20", run_id=2)
        history = merge(merge(empty_history("native"), [old]), [new])
        self.assertEqual(history["points"], 1)
        self.assertEqual(history["series"][0]["run_id"], 2)
        self.assertEqual(history["series"][0]["historical_note"], "keep this")
        self.assertEqual(merge(history, [old]), history)

    def test_separate_platforms_and_all_runs_do_not_collapse(self):
        a, b = record(), record(run_id=2, runner="linux-x86_64")
        self.assertEqual(merge(empty_history("native"), [a, b])["points"], 2)
        a, b = record(hour="10"), record(hour="20", run_id=2)
        self.assertEqual(merge(empty_history("native", all_runs=True), [a, b])["points"], 2)

    def test_migration_preserves_timing_values_and_expired_provenance(self):
        original = {"branch": "main", "points": 1, "custom": "keep",
                    "series": [{"date": "2026-04-01", "timestamp": "2026-04-01T00:00:00Z",
                                "commit": "12345678", "run_id": 1,
                                "cases": {"example": {"elephc_ms": 1.25, "c_ms": 0.5, "php_ms": None}}}]}
        migrated = migrate_native(original)
        variants = migrated["series"][0]["cases"][0]["variants"]
        self.assertEqual(variants["elephc"]["median_ms"], 1.25)
        self.assertEqual(variants["c"]["median_ms"], 0.5)
        self.assertIsNone(variants["php"]["median_ms"])
        self.assertEqual(migrated["legacy_source"]["custom"], "keep")
        self.assertEqual(migrated["series"][0]["commit"], "12345678")
        self.assertEqual(migrated["series"][0]["environment"]["runner"], "unknown")

    def test_malformed_saved_data_never_turns_into_empty_history(self):
        with tempfile.TemporaryDirectory(prefix="elephc-benchmark-test-") as directory:
            path = Path(directory) / "native.json"
            path.write_text("not JSON")
            with self.assertRaises(json.JSONDecodeError):
                load_history(path, "native")
            self.assertEqual(path.read_text(), "not JSON")
            data = merge(empty_history("native"), [record()])
            data["points"] = 99
            atomic_json(path, data)
            with self.assertRaises(ValueError):
                load_history(path, "native")

    def test_incompatible_branch_cannot_contaminate_main(self):
        with tempfile.TemporaryDirectory(prefix="elephc-benchmark-test-") as directory:
            path = Path(directory) / "native.json"
            atomic_json(path, empty_history("native", "feature"))
            with self.assertRaises(ValueError):
                load_history(path, "native")

    def test_invalid_measurements_are_rejected_and_missing_baselines_allowed(self):
        cases = record()["cases"]
        cases[0]["variants"]["php"] = {"median_ms": None, "samples_ms": [], "skipped": True}
        validate_cases("native", cases)
        for value in (float("nan"), float("inf"), -1, True, None):
            invalid = copy.deepcopy(cases)
            invalid[0]["variants"]["elephc"]["median_ms"] = value
            with self.assertRaises(ValueError):
                validate_cases("native", invalid)
        cases[0]["variants"]["elephc"]["samples_ms"] = [float("nan")]
        with self.assertRaises(ValueError):
            validate_cases("native", cases)

    def test_current_artifact_checks_identity_and_every_suite(self):
        sample = record()
        meta = {"id": 1, "created_at": sample["timestamp"], "run_id": 1, "sha": sample["commit"]}
        payload = {"schema_version": 2, "timestamp": sample["timestamp"], "commit": sample["commit"],
                   "branch": "main", "run_id": 1, "environment": sample["environment"],
                   "suites": {suite: {"cases": record(suite)["cases"]} for suite in SUITES}}
        self.assertEqual(set(current_results(payload, meta, "main")), set(SUITES))
        for field, value in (("commit", "bad"), ("branch", "feature"), ("run_id", 99)):
            with self.assertRaises(ValueError):
                current_results({**payload, field: value}, meta, "main")
        with self.assertRaises(ValueError):
            current_results({**payload, "suites": {"native": payload["suites"]["native"]}}, meta, "main")

    def test_legacy_native_and_magician_files_are_both_imported(self):
        meta = {"id": 1, "created_at": "2026-10-10T12:00:00Z", "run_id": 1, "sha": "a" * 40}
        with tempfile.TemporaryDirectory(prefix="elephc-benchmark-test-") as directory:
            for suite, name in (("native", "benchmark-results.json"), ("magician", "magician-benchmark-results.json")):
                variants = record(suite)["cases"][0]["variants"]
                case = {"case": "example", "metadata": {"eval_invocations": 10}}
                case.update({label.replace("-", "_"): result for label, result in variants.items()})
                atomic_json(Path(directory) / name, {"iterations": 3, "warmup": 1, "cases": [case]})
            records = read_artifact(Path(directory), meta, "main")
            self.assertEqual(set(records), {"native", "magician"})
            self.assertEqual(records["magician"]["cases"][0]["metadata"]["eval_invocations"], 10)

    def test_artifact_listing_filters_name_branch_forks_and_expiry(self):
        base = {"id": 1, "name": "benchmark-results", "expired": False, "created_at": "2026-10-10T00:00:00Z",
                "workflow_run": {"id": 1, "head_sha": "a", "head_branch": "main", "repository_id": 1, "head_repository_id": 1}}
        fork = copy.deepcopy(base)
        fork["workflow_run"]["head_repository_id"] = 2
        with patch("fetch.gh", return_value=json.dumps([{"artifacts": [base, fork, {**base, "expired": True}, {**base, "name": "tests"}]}])) as api:
            self.assertEqual(len(list_artifacts("owner/repo", "main")), 1)
            self.assertIn("name=benchmark-results", api.call_args.args[1])

    def test_legacy_manifest_requires_the_imported_artifact_in_history(self):
        b = {"id": 2, "created_at": "2026-10-10T20:00:00Z"}
        history = merge(empty_history("native"), [record(run_id=2)])
        self.assertTrue(cached_import(b, {"2": {"suites": ["native"]}}, {"native": history}))
        self.assertFalse(cached_import(b, {"2": {"suites": ["native"]}}, {"native": empty_history("native")}))

    def test_failed_refresh_preserves_all_files(self):
        with tempfile.TemporaryDirectory(prefix="elephc-benchmark-test-") as directory:
            root = Path(directory)
            for suite in SUITES:
                atomic_json(root / f"{suite}.json", merge(empty_history(suite), [record(suite)]))
            before = {p.name: p.read_bytes() for p in root.iterdir()}
            meta = {"id": 2, "created_at": "2026-10-11T00:00:00Z", "sha": "a", "run_id": 2}
            with patch("fetch.list_artifacts", return_value=[meta]), patch("fetch.download_artifact", side_effect=RuntimeError("download failed")):
                with self.assertRaises(RuntimeError):
                    refresh("owner/repo", "main", root)
            self.assertEqual(before, {p.name: p.read_bytes() for p in root.iterdir()})

    def test_shared_artifact_refresh_is_complete_and_second_refresh_downloads_nothing(self):
        sample = record()
        meta = {"id": 1, "created_at": sample["timestamp"], "sha": sample["commit"], "run_id": 1}
        payload = {"schema_version": 2, "timestamp": sample["timestamp"], "commit": sample["commit"],
                   "branch": "main", "run_id": 1, "run_attempt": 1, "environment": sample["environment"],
                   "suites": {suite: {"cases": record(suite)["cases"]} for suite in SUITES}}
        with tempfile.TemporaryDirectory(prefix="elephc-benchmark-test-") as directory:
            root = Path(directory)

            def download(repo, artifact, destination):
                self.assertEqual(artifact["id"], 1)
                atomic_json(destination / "results.json", payload)

            with patch("fetch.list_artifacts", return_value=[meta]), patch("fetch.download_artifact", side_effect=download) as downloader:
                histories = refresh("owner/repo", "main", root)
                self.assertEqual(downloader.call_count, 1)
                self.assertTrue(all(h["points"] == 1 for h in histories.values()))
                before = {p.name: p.read_bytes() for p in root.iterdir()}
                refresh("owner/repo", "main", root)
                self.assertEqual(downloader.call_count, 1)
                self.assertEqual(before, {p.name: p.read_bytes() for p in root.iterdir()})

    def test_current_raw_samples_must_agree_with_the_reported_median(self):
        invalid = record()
        invalid["cases"][0]["variants"]["elephc"]["median_ms"] = 99
        with self.assertRaises(ValueError):
            merge(empty_history("native"), [invalid])


if __name__ == "__main__":
    unittest.main()
