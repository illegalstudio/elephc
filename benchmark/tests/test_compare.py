"""Verify mixed-workload checksums, phase accounting, sample ordering and noise rejection."""

import unittest
import hashlib
from contextlib import redirect_stderr
from io import StringIO
import tempfile
from pathlib import Path
from unittest.mock import patch

import support  # Bootstrap the benchmark import path for the workflow's unittest discovery command.
from compare import FIELDS, assignments, assert_no_builds, file_hash, main, bridge_environment, bridge_fingerprints, measure_commands, parse_sample, summarize


class CompareTests(unittest.TestCase):
    def test_file_hash_works_without_python_311_file_digest(self):
        with tempfile.TemporaryDirectory(prefix="elephc-compare-hash-") as temporary:
            source = Path(temporary) / "input"
            for data in (b"", b"short", b"abc12345" * (256 * 1024) + b"tail"):
                source.write_bytes(data)
                with patch("compare.hashlib.file_digest", create=True,
                           side_effect=AssertionError("Python 3.11-only hashing API")):
                    self.assertEqual(file_hash(source), hashlib.sha256(data).hexdigest())

    def test_every_compiler_requires_a_revision_before_any_subprocess(self):
        with tempfile.TemporaryDirectory(prefix="elephc-compare-revisions-") as temporary:
            compiler = Path(temporary) / "compiler"
            compiler.write_bytes(b"compiler")
            output = Path(temporary) / "results.json"
            cases = [([], ["a"], "a"), (["a=main-sha"], ["a", "b"], "b")]
            for revisions, labels, missing in cases:
                argv = ["compare.py", "--json", str(output)]
                for label in labels:
                    argv.extend(["--compiler", f"{label}={compiler}"])
                for revision in revisions:
                    argv.extend(["--revision", revision])
                error = StringIO()
                with self.subTest(revisions=revisions), patch("sys.argv", argv), \
                        patch("compare.subprocess.run") as run, redirect_stderr(error):
                    with self.assertRaises(SystemExit) as raised:
                        main()
                    self.assertEqual(raised.exception.code, 2)
                    self.assertIn(f"missing: {missing}", error.getvalue())
                    run.assert_not_called()
                    self.assertFalse(output.exists())

    @patch("compare.execute", return_value="python3\n/usr/bin/rustc\nas\n")
    def test_idle_guard_rejects_builders_even_when_timing_dispersion_would_be_low(self, execute):
        with self.assertRaisesRegex(RuntimeError, "concurrent compilation detected"):
            assert_no_builds(Path("."), 5)

    @patch("compare.execute", return_value="python3\nps\nsshd\n")
    def test_idle_guard_accepts_unrelated_processes(self, execute):
        assert_no_builds(Path("."), 5)

    def test_explicit_bridges_replace_ambient_overrides_and_fingerprint_all_archives(self):
        with tempfile.TemporaryDirectory(prefix="elephc-compare-test-") as temporary:
            directory = Path(temporary)
            for name in ["magician", "tz"]:
                (directory / f"libelephc_{name}.a").write_bytes(name.encode())
            environment = bridge_environment({"PATH": "/bin", "ELEPHC_MAGICIAN_LIB_DIR": "/debug",
                                              "ELEPHC_TZ_LIB_DIR": "/debug", "ELEPHC_PDO_LIB_DIR": "/debug"}, directory)
            self.assertEqual(environment, {"PATH": "/bin", "ELEPHC_MAGICIAN_LIB_DIR": temporary,
                                           "ELEPHC_TZ_LIB_DIR": temporary})
            before = bridge_fingerprints({"main": directory})
            self.assertEqual(set(before["main"]), {"libelephc_magician.a", "libelephc_tz.a"})
            (directory / "libelephc_tz.a").write_bytes(b"changed")
            self.assertNotEqual(before, bridge_fingerprints({"main": directory}))

    def test_variant_options_reject_duplicates_and_missing_values(self):
        self.assertEqual(assignments(["main=/compiler", "release=/release"]),
                         {"main": "/compiler", "release": "/release"})
        for invalid in (["x=a", "x=b"], ["x="], ["=a"], ["x"]):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                assignments(invalid)

    def test_every_sample_requires_correct_output_and_consistent_phase_clocks(self):
        valid = parse_sample("answer|15000000|1000000:2000000:3000000:4000000:5000000", "answer", 20000000)
        self.assertEqual(valid["body"], 15)
        self.assertEqual(valid["wall"], 20)
        for invalid in ("wrong|15|1:2:3:4:5", "answer|16|1:2:3:4:5",
                        "answer|15|1:2:3:4", "answer|13|-1:2:3:4:5"):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                parse_sample(invalid, "answer", 20)
        with self.assertRaises(ValueError):
            parse_sample("answer|15|1:2:3:4:5", "answer", 14)

    def test_noise_gate_is_per_phase_and_retains_a_stable_body(self):
        rows = [{field: 10.0 for field in FIELDS} for _ in range(3)]
        rows[0]["closure"] = 1.0
        rows[2]["closure"] = 30.0
        stats = summarize({"main": rows}, .05)["main"]
        self.assertFalse(stats["closure"]["noise_pass"])
        self.assertTrue(stats["body"]["noise_pass"])
        self.assertEqual(stats["closure"]["median_ms"], 10)

    @patch("compare.parse_sample", return_value={field: 1.0 for field in FIELDS})
    @patch("compare.execute", return_value="checked")
    def test_variants_rotate_and_warmups_do_not_enter_recorded_samples(self, execute, parse):
        samples = measure_commands({"a": ["a"], "b": ["b"], "php": ["php"]},
                                   Path("."), "checksum", 3, 1, 5)
        self.assertEqual([call.args[0][0] for call in execute.call_args_list],
                         ["php", "a", "b", "a", "b", "php", "b", "php", "a", "php", "a", "b"])
        self.assertEqual({label: len(rows) for label, rows in samples.items()}, {"a": 3, "b": 3, "php": 3})
        self.assertEqual(parse.call_count, 12)


if __name__ == "__main__":
    unittest.main()
