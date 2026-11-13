"""Measurements must outlive their disposable Actions artifact without accepting forged data."""

import base64
import copy
import json
import subprocess
import unittest
from unittest.mock import patch

from support import record
from common import SUITES, atomic_json
from archive import BRANCH, decode, list_archives, preserve, read_archive, validate
from test_fetch import artifact


def fixture():
    """Use a complete main benchmark and its independent GitHub run metadata."""
    meta, payload = artifact(record())
    run = dict(id=meta["run_id"], head_sha=meta["sha"], head_branch="main",
               event="push", path=".github/workflows/ci.yml")
    item = dict(id=meta["id"], name="benchmark-results", created_at=meta["created_at"],
                workflow_run={"id": run["id"], "head_sha": run["head_sha"]})
    return run, item, payload


def blob(data):
    """Model the API's base64 JSON envelope."""
    content = json.dumps(data).encode()
    return dict(encoding="base64", size=len(content), content=base64.b64encode(content).decode())


class ArchiveTests(unittest.TestCase):
    """Exercise durable writes, retries, provenance and API failure boundaries."""

    def test_preservation_is_durable_and_idempotent_before_actions_deletion(self):
        run, item, payload = fixture()
        stored, writes = {}, []

        def request(path, method="GET", body=None):
            if method == "GET":
                if not stored:
                    raise subprocess.CalledProcessError(1, ["gh"], stderr="HTTP 404")
                return blob(stored)
            self.assertEqual(body["branch"], BRANCH)
            self.assertEqual(method, "PUT")
            self.assertNotIn("sha", body)  # Immutable artifact IDs are never overwritten.
            stored.update(json.loads(base64.b64decode(body["content"])))
            writes.append(path)

        def download(repo, meta, directory):
            atomic_json(directory / "results.json", payload)

        with patch("archive.download_artifact", side_effect=download) as downloader:
            preserve("owner/repo", run, item, request, apply=True)
            self.assertEqual(len(writes), 1)
            self.assertEqual(set(validate(stored, item["id"])[1]), set(SUITES))
            preserve("owner/repo", run, item, request, apply=True)
            self.assertEqual(downloader.call_count, 1)
            self.assertEqual(len(writes), 1)

    def test_dry_run_validates_without_writes_and_write_errors_propagate(self):
        run, item, payload = fixture()
        calls = []

        def request(path, method="GET", body=None):
            calls.append(method)
            code = 404 if method == "GET" else 403
            raise subprocess.CalledProcessError(1, ["gh"], stderr=f"HTTP {code}")

        with patch("archive.download_artifact", side_effect=lambda repo, meta, directory:
                   atomic_json(directory / "results.json", payload)):
            preserve("owner/repo", run, item, request)
            self.assertEqual(calls, ["GET"])
            with self.assertRaises(subprocess.CalledProcessError):
                preserve("owner/repo", run, item, request, apply=True)
            self.assertEqual(calls, ["GET", "GET", "PUT"])

    def test_untrusted_or_mismatched_sources_never_reach_archive_writes(self):
        run, item, payload = fixture()
        with patch("archive.download_artifact") as download, patch("archive.api") as request:
            for changes in ({"head_branch": "feature"}, {"event": "pull_request"},
                            {"path": ".github/workflows/other.yml"}):
                preserve("owner/repo", dict(run, **changes), item, request, True)
            bad = copy.deepcopy(item)
            bad["workflow_run"]["head_sha"] = "b" * 40
            with self.assertRaisesRegex(ValueError, "inventoried CI run"):
                preserve("owner/repo", run, bad, request, True)
            request.assert_not_called()
            download.assert_not_called()

    def test_saved_records_are_bound_to_the_requested_artifact_identity(self):
        sample = record()
        meta, _ = artifact(sample)
        data = dict(schema=1, branch="main", artifact=meta, records={"native": sample})
        validate(data, meta["id"])
        for field, value in (("commit", "b" * 40), ("run_id", 99), ("artifact_id", 99)):
            bad = copy.deepcopy(data)
            bad["records"]["native"][field] = value
            with self.assertRaisesRegex(ValueError, "does not match"):
                validate(bad, meta["id"])
        run, item, _ = fixture()
        bad = copy.deepcopy(data)
        bad["artifact"]["sha"] = "b" * 40
        bad["records"]["native"]["commit"] = "b" * 40
        with patch("archive.api", return_value=blob(bad)) as request:
            with self.assertRaisesRegex(ValueError, "identity differs"):
                preserve("owner/repo", run, item, request, True)
            self.assertEqual(request.call_count, 1)

    def test_inventory_and_reads_use_immutable_blobs_and_reject_truncation(self):
        root = {"tree": [{"path": "benchmark-results", "type": "tree", "sha": "tree-sha"}]}
        tree = {"tree": [{"path": "1.json", "type": "blob", "sha": "blob-sha", "size": 500}]}
        meta, _ = artifact(record())
        data = dict(schema=1, branch="main", artifact=meta, records={"native": record()})
        with patch("archive.api", side_effect=[root, tree, blob(data)]) as request:
            entries = list_archives("owner/repo", "main")
            self.assertEqual(read_archive("owner/repo", entries[0])[0], meta)
            self.assertEqual(request.call_args.args[1], "git/blobs/blob-sha")
        for response in (dict(root, truncated=True),):
            with patch("archive.api", return_value=response):
                with self.assertRaisesRegex(ValueError, "Truncated"):
                    list_archives("owner/repo", "main")
        with patch("archive.api", side_effect=[root, dict(tree, truncated=True)]):
            with self.assertRaisesRegex(ValueError, "Truncated"):
                list_archives("owner/repo", "main")
        with self.assertRaisesRegex(ValueError, "size"):
            decode(dict(blob(data), size=5 * 1024 * 1024))

    def test_only_confirmed_absence_means_no_durable_archive(self):
        for code in (404, 403, 502):
            error = subprocess.CalledProcessError(1, ["gh"], stderr=f"HTTP {code}")
            with patch("archive.api", side_effect=error):
                if code == 404:
                    self.assertEqual(list_archives("owner/repo", "main"), [])
                else:
                    with self.assertRaises(subprocess.CalledProcessError):
                        list_archives("owner/repo", "main")


if __name__ == "__main__":
    unittest.main()
