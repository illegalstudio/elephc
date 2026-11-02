#!/usr/bin/env python3
"""Exercise the composite action's real shell steps with container-mapped paths."""

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[2]
ACTION_SPEC = REPO / ".github/actions/download-test-archive/action.yml"
VALIDATOR = ACTION_SPEC.with_name("validate.sh")


class ContainerPaths(unittest.TestCase):
    """Keep host contexts distinct from paths visible to the running shell."""

    @classmethod
    def setUpClass(cls):
        """Read the actual action through the YAML parser already used in CI."""
        cls.steps = json.loads(subprocess.check_output([
            "ruby", "-ryaml", "-rjson", "-e",
            "puts JSON.generate(YAML.load_file(ARGV.fetch(0)))", str(ACTION_SPEC),
        ], text=True))["runs"]["steps"]

    def setUp(self):
        """Create isolated host and container views, including paths with spaces."""
        tmp = tempfile.TemporaryDirectory(prefix=".nextest-container-test.", dir=Path.cwd())
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.action = self.root / "container action"
        self.artifacts = self.root / "container artifacts"
        self.host_artifacts = self.root / "host artifacts"
        self.bin = self.root / "bin"
        for directory in [self.action, self.artifacts, self.host_artifacts, self.bin]:
            directory.mkdir()
        shutil.copyfile(VALIDATOR, self.action / "validate.sh")
        self.frame = subprocess.check_output(
            ["zstd", "--quiet", "--stdout"], input=b"complete nextest fixture\n",
        )
        self.archive = self.artifacts / "nextest-archive.tar.zst"
        self.host_archive = self.host_artifacts / self.archive.name
        self.archive.write_bytes(self.frame)
        self.host_archive.write_bytes(self.frame)
        sleep = self.bin / "sleep"
        sleep.write_text("#!/bin/sh\nexit 0\n")
        sleep.chmod(0o755)

    def run_step(self, step, download_path):
        """Resolve contexts, then execute the unmodified action shell body."""
        contexts = {
            "github.action_path": str(self.root / "unmounted host action"),
            "inputs.path": str(self.host_artifacts),
            "inputs.name": "nextest-archive-codegen-linux-aarch64",
            **{f"steps.{name}.outputs.download-path": download_path
               for name in ["first", "second", "third"]},
        }

        def resolve(value):
            """Resolve only the contexts used by this action's shell steps."""
            return re.sub(r"\$\{\{\s*(.*?)\s*\}\}", lambda m: contexts[m[1]], value)

        env = os.environ.copy()
        env.update(GITHUB_ACTION_PATH=str(self.action), RM_LOG=str(self.root / "rm.log"))
        env["PATH"] = str(self.bin) + os.pathsep + env["PATH"]
        env.update({key: resolve(value) for key, value in step.get("env", {}).items()})
        return subprocess.run(
            ["bash", "-e", "-o", "pipefail", "-c", resolve(step["run"])],
            env=env, cwd=self.root, capture_output=True, text=True,
        )

    def test_validation_uses_mapped_action_and_download_paths(self):
        """All three validation steps read the downloaded file inside the container."""
        steps = [s for s in self.steps if "validate.sh" in s.get("run", "")]
        self.assertEqual(len(steps), 3)
        for step in steps:
            self.archive.write_bytes(self.frame)
            result = self.run_step(step, str(self.artifacts))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.archive.write_bytes(self.frame[:-1])
            result = self.run_step(step, str(self.artifacts))
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(self.host_archive.read_bytes(), self.frame)

    def test_cleanup_removes_only_the_actual_download(self):
        """Both retries remove the mapped archive and preserve host-context files."""
        steps = [s for s in self.steps if "rm -f --" in s.get("run", "")]
        self.assertEqual(len(steps), 2)
        for step in steps:
            self.archive.write_bytes(self.frame)
            result = self.run_step(step, str(self.artifacts))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(self.archive.exists())
            self.assertEqual(self.host_archive.read_bytes(), self.frame)

    def test_cleanup_skips_an_absent_download_output(self):
        """An API failure without an output path must never attempt a removal."""
        rm = self.bin / "rm"
        rm.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$RM_LOG"\n')
        rm.chmod(0o755)
        for step in self.steps:
            if "rm -f --" in step.get("run", ""):
                result = self.run_step(step, "")
                self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.root / "rm.log").exists())
        self.assertEqual(self.archive.read_bytes(), self.frame)
        self.assertEqual(self.host_archive.read_bytes(), self.frame)


if __name__ == "__main__":
    unittest.main()
