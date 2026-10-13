"""Exercise publication against local remotes, including a concurrent main update."""

import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from support import record
from common import SUITES, atomic_json
from history import empty_history, merge
from publish import git, publish
from report import generate


class PublishTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="elephc-benchmark-publish-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.remote = self.root / "remote.git"
        git(self.root, "init", "--bare", "--initial-branch=main", str(self.remote))
        self.source = self.root / "source"
        git(self.root, "clone", str(self.remote), str(self.source))
        git(self.source, "config", "user.name", "Benchmark Test")
        git(self.source, "config", "user.email", "benchmark-test@example.invalid")
        for suite in SUITES:
            atomic_json(self.source / "benchmark" / "data" / f"{suite}.json", empty_history(suite))
        atomic_json(self.source / "benchmark" / "data" / "imports.json", {})
        atomic_json(self.source / ".github" / "traffic" / "clones.json", {"total_uniques": 0, "daily": {}})
        generate(self.source / "benchmark" / "data", self.source / "benchmark" / "assets")
        git(self.source, "add", ".")
        git(self.source, "commit", "-m", "test: initialize generated history")
        git(self.source, "push", "origin", "main")
        self.initial = git(self.source, "rev-parse", "HEAD").stdout.strip()
        atomic_json(self.source / "benchmark" / "data" / "native.json",
                    merge(empty_history("native"), [record(day="2026-10-10")]))

    def remote_history(self):
        return json.loads(git(self.root, "--git-dir", str(self.remote), "show", "main:benchmark/data/native.json").stdout)

    def test_normal_publication_is_idempotent_and_keeps_source_checkout_untouched(self):
        (self.source / "unrelated.txt").write_text("never publish me")
        self.assertTrue(publish(self.source))
        self.assertEqual(self.remote_history()["points"], 1)
        self.assertFalse(publish(self.source))
        self.assertEqual(git(self.source, "rev-parse", "HEAD").stdout.strip(), self.initial)
        files = git(self.root, "--git-dir", str(self.remote), "ls-tree", "-r", "--name-only", "main").stdout
        self.assertNotIn("unrelated.txt", files)
        message = git(self.root, "--git-dir", str(self.remote), "log", "-1", "--format=%s").stdout
        self.assertIn("[skip ci]", message)

    def test_checkout_included_authentication_reaches_the_temporary_checkout(self):
        config = self.root / "checkout-credentials.config"
        header = "AUTHORIZATION: basic dGVzdDppbnZhbGlk"
        git(self.root, "config", "--file", str(config), "http.https://github.com/.extraheader", header)
        git(self.source, "config", "--local", "include.path", str(config))
        direct = git(self.source, "config", "--local", "--get-regexp", "extraheader", check=False)
        self.assertEqual(direct.returncode, 1)
        with patch("publish.git", wraps=git) as commands:
            self.assertTrue(publish(self.source))
        copied = [call.args[1:] for call in commands.call_args_list
                  if call.args[1:5] == ("config", "--local", "--add", "http.https://github.com/.extraheader")]
        self.assertEqual(copied, [("config", "--local", "--add", "http.https://github.com/.extraheader", header)])

    def test_racing_main_commit_preserves_both_days_and_generated_reports(self):
        writer = self.root / "writer"
        git(self.root, "clone", str(self.remote), str(writer))
        git(writer, "config", "user.name", "Concurrent Test")
        git(writer, "config", "user.email", "concurrent-test@example.invalid")
        raced = False
        original_git = git

        def concurrent_git(cwd, *arguments, **kwargs):
            nonlocal raced
            if arguments[0] == "push" and not raced:
                raced = True
                atomic_json(writer / "benchmark" / "data" / "native.json",
                            merge(empty_history("native"), [record(day="2026-10-11", run_id=2)]))
                original_git(writer, "add", "benchmark/data/native.json")
                original_git(writer, "commit", "-m", "test: add concurrent sample")
                original_git(writer, "push", "origin", "main")
            return original_git(cwd, *arguments, **kwargs)

        with patch("publish.git", side_effect=concurrent_git):
            self.assertTrue(publish(self.source))
        self.assertTrue(raced)
        history = self.remote_history()
        self.assertEqual([r["date"] for r in history["series"]], ["2026-10-10", "2026-10-11"])
        svg = git(self.root, "--git-dir", str(self.remote), "show", "main:benchmark/assets/native-light.svg").stdout
        self.assertIn("Latest: 2026-10-11", svg)


if __name__ == "__main__":
    unittest.main()
