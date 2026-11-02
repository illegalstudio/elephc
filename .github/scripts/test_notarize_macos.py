"""Exercise the release notarization gate without Apple credentials or macOS tools."""

import base64
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest
import zipfile


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / ".github/actions/notarize-macos/notarize.sh"
# The runner's own keychains, including a path with a space for the parser.
SEARCH_LIST = ["/Users/runner/Library/Keychains/login.keychain-db", "/Library/Keychains/Fixture Keychain.keychain-db"]


def read_yaml(path):
    """Use the same standard-library YAML parser as the CI Action pin gate."""
    result = subprocess.run(
        ["ruby", "-ryaml", "-rjson", "-e", "puts JSON.generate(YAML.load_file(ARGV.fetch(0)))", str(path)],
        capture_output=True, text=True, check=True,
    )
    return json.loads(result.stdout)


MOCK_TOOL = r'''
import json
import os
from pathlib import Path
import signal
import sys
import time
import zipfile

tool = Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ["MOCK_LOG"], "a") as log:
    log.write(json.dumps({"tool": tool, "args": args}) + "\n")

if tool == "security":
    if args[0] == "create-keychain":
        Path(args[-1]).touch()
    if args[0] == "import" and os.environ.get("MOCK_IMPORT_FAIL"):
        sys.exit(1)
    if args[0] == "delete-keychain":
        Path(args[-1]).unlink(missing_ok=True)
    if args[0] == "list-keychains":
        search_list = Path(os.environ["MOCK_SEARCH_LIST"])
        if "-s" in args:
            search_list.write_text(json.dumps(args[args.index("-s") + 1:]))
        else:
            for keychain in json.loads(search_list.read_text()):
                print(f'    "{keychain}"')
elif tool == "codesign":
    if os.environ.get("MOCK_CODESIGN_FAIL") == args[0]:
        sys.exit(1)
    if args[0] == "--force":
        # Like the real tool, --keychain only filters the user search list.
        keychain = args[args.index("--keychain") + 1]
        if keychain not in json.loads(Path(os.environ["MOCK_SEARCH_LIST"]).read_text()):
            print("error: The specified item could not be found in the keychain.", file=sys.stderr)
            sys.exit(1)
        binary = Path(args[-1])
        binary.write_bytes(binary.read_bytes() + b"signed-by-fixture\n")
elif tool == "ditto":
    payload_dir = Path(args[-2])
    assert payload_dir.is_dir(), "Notarization requires a directory with an explicit ZIP layout"
    with zipfile.ZipFile(args[-1], "w") as archive:
        for binary in payload_dir.iterdir():
            archive.write(binary, str(Path(payload_dir.name) / binary.name))
elif tool == "xcrun":
    assert args[0] == "notarytool", args
    if args[1] == "submit":
        with zipfile.ZipFile(args[2]) as archive:
            assert archive.namelist() == ["payload/elephc"], archive.namelist()
            payload = archive.read("payload/elephc")
        assert payload.endswith(b"signed-by-fixture\n"), payload
        assert payload == Path(os.environ["NOTARIZE_BINARY"]).read_bytes()
        if os.environ.get("MOCK_TERMINATE"):
            if os.environ.get("MOCK_CHILD_STAYS_ALIVE"):
                signal.signal(signal.SIGTERM, signal.SIG_IGN)
                Path(os.environ["MOCK_CHILD_PID_FILE"]).write_text(str(os.getpid()))
            os.kill(int(os.environ.get("MOCK_STEP_PID", os.getppid())), signal.SIGTERM)
            if os.environ.get("MOCK_CHILD_STAYS_ALIVE"):
                time.sleep(60)
            sys.exit(1)
        if os.environ.get("MOCK_BAD_JSON"):
            print("unexpected response")
        else:
            print(json.dumps({"id": "fixture-submission", "status": os.environ.get("MOCK_STATUS", "Accepted")}))
        sys.exit(int(os.environ.get("MOCK_SUBMIT_EXIT", "0")))
    assert args[1] == "log", args
    if os.environ.get("MOCK_LOG_FAIL"):
        sys.exit(1)
    Path(args[-1]).write_text(json.dumps({"issues": [{"message": "fixture rejection details"}]}))
else:
    raise AssertionError(tool)
'''


class NotarizationTests(unittest.TestCase):
    """The gate must require acceptance and clean credentials on every exit path."""

    @classmethod
    def setUpClass(cls):
        # One suite-owned system temp directory keeps interrupted fixtures out
        # of repository scans without adding tool-specific ignore entries.
        cls.suite_scratch = tempfile.TemporaryDirectory(prefix="elephc-notarization-tests-")
        cls.addClassCleanup(cls.suite_scratch.cleanup)

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="case-", dir=self.suite_scratch.name)
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.runner_temp = self.root / "runner-temp"
        self.runner_temp.mkdir()
        self.log = self.root / "commands.jsonl"
        self.search_list = self.root / "search-list.json"
        self.search_list.write_text(json.dumps(SEARCH_LIST))
        self.binary = self.root / "elephc"
        self.binary.write_text("#!/bin/sh\necho fixture\n")
        self.binary.chmod(0o755)
        mock_bin = self.root / "bin"
        mock_bin.mkdir()
        mock = mock_bin / "mock-tool"
        mock.write_text(f"#!{sys.executable}\n{MOCK_TOOL}")
        mock.chmod(0o755)
        for tool in ("security", "codesign", "ditto", "xcrun"):
            (mock_bin / tool).symlink_to(mock)
        self.env = {
            **os.environ,
            "PATH": f"{mock_bin}{os.pathsep}{os.environ['PATH']}",
            "RUNNER_TEMP": str(self.runner_temp),
            "NOTARIZE_BINARY": str(self.binary),
            "MOCK_LOG": str(self.log),
            "MOCK_SEARCH_LIST": str(self.search_list),
            "APPLE_CERTIFICATE_P12": base64.b64encode(b"fixture certificate").decode(),
            "APPLE_CERTIFICATE_PASSWORD": "fixture-certificate-password",
            "APPLE_SIGNING_IDENTITY": "Developer ID Application: Fixture",
            "APPLE_ID": "fixture@example.invalid",
            "APPLE_TEAM_ID": "FIXTURE",
            "APPLE_APP_PASSWORD": "fixture-app-password",
        }

    def run_gate(self, *, command=None, cwd=None, **overrides):
        """Run the real shell entry point and require removal of its scratch data."""
        result = subprocess.run(
            command or ["bash", str(SCRIPT)], cwd=cwd, env={**self.env, **overrides},
            capture_output=True, text=True, timeout=20,
        )
        self.assertEqual(list(self.runner_temp.iterdir()), [], result.stdout + result.stderr)
        self.assertEqual(json.loads(self.search_list.read_text()), SEARCH_LIST, "Runner search list must be restored")
        calls = [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []
        if calls:
            self.assertEqual(calls[-1]["tool"], "security")
            self.assertEqual(calls[-1]["args"][0], "delete-keychain")
        return result, calls

    def test_accepted_signed_binary_passes(self):
        result, calls = self.run_gate()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("Apple accepted notarization submission fixture-submission", result.stdout)
        sign = next(call for call in calls if call["tool"] == "codesign")
        self.assertIn("runtime", sign["args"])
        self.assertIn("--timestamp", sign["args"])
        self.assertIn("--keychain", sign["args"])
        keychain = next(call["args"][-1] for call in calls if call["args"][0] == "create-keychain")
        self.assertIn(["list-keychains", "-d", "user", "-s", keychain, *SEARCH_LIST], [call["args"] for call in calls])
        steps = [(call["tool"], call["args"][0]) for call in calls]
        self.assertLess(steps.index(("security", "find-identity")), steps.index(("codesign", "--force")))

    def test_missing_secret_fails_before_import(self):
        result, calls = self.run_gate(APPLE_ID="")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Missing required notarization setting: APPLE_ID", result.stderr)
        self.assertEqual(calls, [])

    def test_missing_binary_fails_before_import(self):
        result, calls = self.run_gate(NOTARIZE_BINARY=str(self.root / "missing"))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Notarization requires an executable", result.stderr)
        self.assertEqual(calls, [])

    def test_invalid_certificate_cleans_scratch_before_import(self):
        result, calls = self.run_gate(APPLE_CERTIFICATE_P12="!invalid-base64!")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(call["tool"] == "xcrun" for call in calls))

    def test_import_and_signature_failures_prevent_submission(self):
        for failure in ({"MOCK_IMPORT_FAIL": "1"}, {"MOCK_CODESIGN_FAIL": "--force"}, {"MOCK_CODESIGN_FAIL": "--verify"}):
            with self.subTest(failure=failure):
                self.log.unlink(missing_ok=True)
                result, calls = self.run_gate(**failure)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(any(call["tool"] == "xcrun" for call in calls))

    def test_rejection_with_zero_exit_still_fails_and_prints_diagnostics(self):
        result, _ = self.run_gate(MOCK_STATUS="Invalid")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("fixture rejection details", result.stdout)
        self.assertIn("requires Accepted status", result.stderr)

    def test_timeout_fails(self):
        result, _ = self.run_gate(MOCK_STATUS="In Progress", MOCK_SUBMIT_EXIT="75")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("exit 75", result.stderr)

    def test_nonzero_exit_cannot_report_success(self):
        result, _ = self.run_gate(MOCK_SUBMIT_EXIT="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("Apple accepted notarization", result.stdout)

    def test_malformed_response_fails(self):
        result, _ = self.run_gate(MOCK_BAD_JSON="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("got 'unknown'", result.stderr)

    def test_unavailable_diagnostic_log_preserves_failure(self):
        result, _ = self.run_gate(MOCK_STATUS="Invalid", MOCK_LOG_FAIL="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("requires Accepted status", result.stderr)

    def test_termination_cleans_credentials(self):
        result, _ = self.run_gate(MOCK_TERMINATE="1")
        self.assertEqual(result.returncode, 143)
        self.assertNotIn("Apple accepted notarization", result.stdout)

    def test_runner_cancellation_reaches_cleanup_through_action_wrapper(self):
        action = read_yaml(ROOT / ".github/actions/notarize-macos/action.yml")
        # The runner signals the wrapper PID, which exec must preserve for the
        # actual script, rather than leaving an orphaned foreground child.
        wrapper = 'export MOCK_STEP_PID="$$"\n' + action["runs"]["steps"][0]["run"]
        wrapper_path = self.root / "runner-step.sh"
        wrapper_path.write_text(wrapper)
        result, _ = self.run_gate(
            command=["bash", "--noprofile", "--norc", "-e", "-o", "pipefail", str(wrapper_path)],
            GITHUB_ACTION_PATH=str(SCRIPT.parent), MOCK_TERMINATE="1",
        )
        self.assertEqual(result.returncode, 143, result.stdout + result.stderr)
        self.assertNotIn("Apple accepted notarization", result.stdout)

    def test_cancellation_stops_live_notary_child_and_cleans_promptly(self):
        action = read_yaml(ROOT / ".github/actions/notarize-macos/action.yml")
        wrapper_path = self.root / "runner-step.sh"
        wrapper_path.write_text('export MOCK_STEP_PID="$$"\n' + action["runs"]["steps"][0]["run"])
        child_pid_file = self.root / "notary-child.pid"
        process = subprocess.Popen(
            ["bash", "--noprofile", "--norc", "-e", "-o", "pipefail", str(wrapper_path)],
            env={**self.env, "GITHUB_ACTION_PATH": str(SCRIPT.parent), "MOCK_TERMINATE": "1",
                 "MOCK_CHILD_STAYS_ALIVE": "1", "MOCK_CHILD_PID_FILE": str(child_pid_file)},
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        started = time.monotonic()
        try:
            stdout, stderr = process.communicate(timeout=5)
            self.assertEqual(process.returncode, 143, stdout + stderr)
            self.assertLess(time.monotonic() - started, 5)
            self.assertEqual(list(self.runner_temp.iterdir()), [], stdout + stderr)
            self.assertEqual(json.loads(self.search_list.read_text()), SEARCH_LIST)
            self.assertTrue(child_pid_file.is_file(), "Worker must remain alive after signalling its parent")
            with self.assertRaises(ProcessLookupError):
                os.kill(int(child_pid_file.read_text()), 0)
            calls = [json.loads(line) for line in self.log.read_text().splitlines()]
            self.assertEqual(calls[-1]["tool"], "security")
            self.assertEqual(calls[-1]["args"][0], "delete-keychain")
        finally:
            # A failed regression must also terminate its deliberately stuck
            # child so it cannot hold stdout pipes or outlive the test suite.
            if process.poll() is None:
                process.kill()
            if child_pid_file.is_file():
                try:
                    os.kill(int(child_pid_file.read_text()), signal.SIGKILL)
                except ProcessLookupError:
                    pass
            process.communicate(timeout=5)

    def test_nightly_can_sign_a_candidate_without_the_action(self):
        workflow = read_yaml(ROOT / ".github/workflows/nightly.yml")
        steps = workflow["jobs"]["build"]["steps"]
        sign = next(step for step in steps if step.get("name") == "Sign and notarize macOS binary")
        load = next(step for step in steps if step.get("name") == "Load macOS notarization action")
        self.assertEqual(steps[0]["with"]["ref"], "${{ needs.gate.outputs.sha }}")
        self.assertEqual(load["with"]["ref"], "${{ github.workflow_sha }}")
        self.assertEqual(load["if"], "runner.os == 'macOS'")
        self.assertFalse(load["with"]["persist-credentials"])
        self.assertLess(steps.index(load), steps.index(sign))
        action_dir = self.root / sign["uses"]
        action_dir.relative_to(self.root / load["with"]["path"])

        # This represents the old green compiler tree, without the new action.
        compiler_source = self.root / "candidate-source"
        compiler_source.write_text("selected green compiler revision\n")
        candidate_binary = self.root / sign["with"]["binary"]
        candidate_binary.parent.mkdir(parents=True)
        self.binary.rename(candidate_binary)
        self.assertFalse((self.root / ".github/actions/notarize-macos").exists())
        # The second checkout supplies only CI tooling from the workflow SHA.
        shutil.copytree(ROOT / ".github/actions/notarize-macos", action_dir)
        action = read_yaml(action_dir / "action.yml")
        action_step = action["runs"]["steps"][0]
        inputs = {}
        for key, value in sign["with"].items():
            secret = re.fullmatch(r"\$\{\{ secrets\.(\w+) \}\}", value)
            inputs[key] = self.env[secret[1]] if secret else value
        action_env = {}
        for key, expression in action_step["env"].items():
            input_name = re.fullmatch(r"\$\{\{ inputs\.([\w-]+) \}\}", expression)
            self.assertIsNotNone(input_name, expression)
            action_env[key] = inputs[input_name[1]]
        result, _ = self.run_gate(
            command=["bash", "-e", "-o", "pipefail", "-c", action_step["run"]],
            cwd=self.root, GITHUB_ACTION_PATH=str(action_dir), **action_env,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(compiler_source.read_text(), "selected green compiler revision\n")

    def test_workflows_gate_packaging_and_publication(self):
        for name in ("release", "nightly"):
            with self.subTest(workflow=name):
                workflow = read_yaml(ROOT / f".github/workflows/{name}.yml")
                steps = workflow["jobs"]["build"]["steps"]
                sign = next(step for step in steps if step.get("name") == "Sign and notarize macOS binary")
                self.assertEqual(sign["if"], "runner.os == 'macOS'")
                self.assertFalse(sign.get("continue-on-error", False))
                names = [step.get("name") for step in steps]
                order = [names.index(n) for n in (
                    "Strip binary", "Sign and notarize macOS binary", "Create tarball",
                    "Verify packaged macOS signature", f"Upload {name} artifact",
                )]
                self.assertEqual(order, sorted(order))
                upload = steps[order[-1]]
                self.assertNotIn("if", upload)  # Default success() must gate uploads.
                self.assertIn("verify-artifact", workflow["jobs"]["release" if name == "release" else "publish"]["needs"])
        ci = read_yaml(ROOT / ".github/workflows/ci.yml")
        for job in ci["jobs"].values():
            for step in job.get("steps", []):
                self.assertNotIn("notarize-macos", step.get("uses", ""))

    def test_packaged_signature_gate_rejects_changed_or_invalid_binaries(self):
        binary = self.root / "target/release/elephc"
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b"signed fixture\n")
        archive_path = self.root / "elephc.tar.gz"
        with tarfile.open(archive_path, "w:gz") as archive:
            archive.add(binary, arcname="elephc")
        for name in ("release", "nightly"):
            workflow = read_yaml(ROOT / f".github/workflows/{name}.yml")
            step = next(s for s in workflow["jobs"]["build"]["steps"] if s.get("name") == "Verify packaged macOS signature")
            self.assertEqual(step["if"], "runner.os == 'macOS'")
            for mode in ("valid", "different-bytes", "invalid-signature"):
                with self.subTest(workflow=name, mode=mode):
                    self.log.unlink(missing_ok=True)
                    binary.write_bytes(b"different fixture\n" if mode == "different-bytes" else b"signed fixture\n")
                    result = subprocess.run(
                        ["bash", "-e", "-o", "pipefail", "-c", step["run"]],
                        cwd=self.root, env={**self.env, "TARBALL": str(archive_path),
                                           "MOCK_CODESIGN_FAIL": "--verify" if mode == "invalid-signature" else ""},
                        capture_output=True, text=True, timeout=20,
                    )
                    self.assertEqual(result.returncode == 0, mode == "valid", result.stdout + result.stderr)
                    self.assertEqual(list(self.runner_temp.iterdir()), [])
                    if mode == "different-bytes":
                        self.assertFalse(self.log.exists(), "Modified bytes must fail before signature verification")

    @unittest.skipUnless(sys.platform == "darwin", "Requires native macOS signing and archive tools")
    def test_native_macos_signature_survives_zip_and_tar(self):
        """Exercise actual platform tools without Developer ID secrets or Apple submission."""
        source = self.root / "probe.c"
        source.write_text("int main(void) { return 0; }\n")
        subprocess.run(["/usr/bin/cc", str(source), "-o", str(self.binary)], check=True, capture_output=True)
        subprocess.run([
            "/usr/bin/codesign", "--force", "--options", "runtime", "--timestamp=none",
            "--sign", "-", str(self.binary),
        ], check=True, capture_output=True)
        subprocess.run([str(self.binary)], check=True, capture_output=True)
        archive_path = self.root / "native.zip"
        payload_dir = self.root / "payload"
        payload_dir.mkdir()
        shutil.copy2(self.binary, payload_dir / "elephc")
        subprocess.run([
            "/usr/bin/ditto", "-c", "-k", "--keepParent", str(payload_dir), str(archive_path),
        ], check=True, capture_output=True)
        with zipfile.ZipFile(archive_path) as archive:
            self.assertIn("payload/elephc", archive.namelist(), archive.namelist())
            self.assertEqual(archive.read("payload/elephc"), self.binary.read_bytes())
        tar_path = self.root / "native.tar.gz"
        subprocess.run([
            "/usr/bin/tar", "czf", str(tar_path), "-C", str(self.root), "elephc",
        ], check=True, capture_output=True)
        extract = self.root / "extracted"
        extract.mkdir()
        subprocess.run([
            "/usr/bin/tar", "xzf", str(tar_path), "-C", str(extract), "elephc",
        ], check=True, capture_output=True)
        self.assertEqual((extract / "elephc").read_bytes(), self.binary.read_bytes())
        subprocess.run([
            "/usr/bin/codesign", "--verify", "--strict", "--verbose=2", str(extract / "elephc"),
        ], check=True, capture_output=True)


if __name__ == "__main__":
    unittest.main()
