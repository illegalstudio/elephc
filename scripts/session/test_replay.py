#!/usr/bin/env python3
"""Guard PHPT extraction and INI quoting against false oracle failures."""
import shutil
import subprocess
import unittest
from unittest import mock
from types import SimpleNamespace
import tempfile
from pathlib import Path

import replay


class ReplayTests(unittest.TestCase):
    """Exercise actual corruption cases encountered while replaying upstream sessions."""

    def test_binary_file_section_is_preserved(self):
        """Embedded NULs and boundary-looking indented text must remain source bytes."""
        body = b"<?php echo \"\x00\";\n --EXPECT--\n"
        parts = replay.sections(b"--TEST--\nx\n--FILE--\n" + body + b"--EXPECT--\nx\n")
        self.assertEqual(parts["FILE"], body)

    def test_duplicate_sections_are_rejected(self):
        """A second EXPECT section must not silently replace the scored expectation."""
        with self.assertRaises(ValueError):
            replay.sections(b"--TEST--\nx\n--EXPECT--\na\n--EXPECT--\nb\n")

    @unittest.skipUnless(shutil.which("php"), "PHP is required to test its INI argument grammar")
    def test_quoted_ini_values_reach_php_unchanged(self):
        """Semicolons and equals inside a quoted save path must not become INI syntax."""
        settings = replay.ini_settings({"INI": b'session.save_path="path;with=literal;bytes"\n'}, Path.cwd())
        args = replay.php_args(Path(shutil.which("php")), settings, Path("unused"))[:-1]
        result = subprocess.run(args + ["-r", 'echo ini_get("session.save_path");'], capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, b"path;with=literal;bytes")
        self.assertEqual(result.stderr, b"")

    @unittest.skipUnless(shutil.which("php"), "PHP is required for a real exit-status probe")
    def test_expected_nonzero_reference_exit_reaches_native_phase(self):
        """Upstream PHPT fatal-error expectations must not become oracle errors for exit 255."""
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixtures = root / "source/ext/session/tests"
            fixtures.mkdir(parents=True)
            case = fixtures / "fatal.phpt"
            case.write_bytes(b'--TEST--\nexpected nonzero exit\n--FILE--\n'
                             b'<?php echo "expected fatal\\n"; exit(255);\n'
                             b'--EXPECT--\nexpected fatal\n')
            options = SimpleNamespace(php_src=root / "source", output=root / "evidence",
                                      php=Path(shutil.which("php")), elephc=Path("/usr/bin/true"),
                                      project=root, timeout=10)
            native_result = {"exit": 0, "http_status": 200, "headers": [],
                             "output": b"expected fatal\n"}
            def native(*args):
                (args[-1] / "native.server.log").write_bytes(b"")
                return native_result
            with mock.patch.object(replay, "matches", side_effect=lambda php, matcher, kind, want, got:
                                   want.strip() == got.strip()), mock.patch.object(
                                       replay, "native_http", side_effect=native) as request:
                result = replay.replay_case(case, options, root / "unused-matcher", set())
            self.assertEqual(result["reference"]["exit"], 255)
            self.assertEqual(result["status"], "pass", result)
            request.assert_called_once()
            # A compiled worker dropping its response is runtime evidence, not a missing adapter.
            options.output = root / "evidence-disconnect"
            with mock.patch.object(replay, "matches", return_value=True), mock.patch.object(
                    replay, "native_http", side_effect=replay.http.client.RemoteDisconnected("worker closed")):
                disconnected = replay.replay_case(case, options, root / "unused-matcher", set())
            self.assertEqual(disconnected["status"], "runtime_http_error", disconnected)

    @unittest.skipUnless(shutil.which("php"), "PHP is required for open_basedir semantics")
    def test_temp_directory_stays_outside_test_cwd(self):
        """Private temp isolation must preserve the oracle's open_basedir=. restriction."""
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixtures = root / "source/ext/session/tests"
            fixtures.mkdir(parents=True)
            case = fixtures / "basedir.phpt"
            case.write_bytes(b'--TEST--\ntemp outside cwd\n--INI--\nopen_basedir=.\n'
                             b'--FILE--\n<?php echo @is_dir(sys_get_temp_dir()) ? "inside" : "outside";\n'
                             b'--EXPECT--\noutside\n')
            options = SimpleNamespace(php_src=root / "source", output=root / "evidence",
                                      php=Path(shutil.which("php")), elephc=Path("/usr/bin/true"),
                                      project=root, timeout=10)
            def native(*args):
                (args[-1] / "native.server.log").write_bytes(b"")
                return {"exit": 0, "http_status": 200, "headers": [], "output": b"outside"}
            with mock.patch.object(replay, "matches", side_effect=lambda php, matcher, kind, want, got:
                                   want.strip() == got.strip()), mock.patch.object(
                                       replay, "native_http", side_effect=native):
                result = replay.replay_case(case, options, root / "unused-matcher", set())
            self.assertEqual(result["status"], "pass", result)


if __name__ == "__main__":
    unittest.main()
