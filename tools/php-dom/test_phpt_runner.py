#!/usr/bin/env python3
"""Focused unit and integration tests for the PHP DOM PHPT harness."""

from __future__ import annotations

import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("phpt_runner.py")
SPEC = importlib.util.spec_from_file_location("php_dom_phpt_runner", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
RUNNER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = RUNNER
SPEC.loader.exec_module(RUNNER)


class PhptParserTests(unittest.TestCase):
    """Exercise byte-preserving PHPT section parsing and validation."""

    def setUp(self) -> None:
        """Create one disposable directory for PHPT fixtures."""
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)

    def tearDown(self) -> None:
        """Remove the disposable PHPT fixture directory."""
        self.temporary.cleanup()

    def write_case(self, payload: bytes) -> Path:
        """Write one raw PHPT payload and return its path."""
        path = self.root / "case.phpt"
        path.write_bytes(payload)
        return path

    def test_file_and_expect_are_preserved_as_bytes(self) -> None:
        """Parse ordinary FILE/EXPECT plus all requested control sections."""
        case = RUNNER.parse_phpt(
            self.write_case(
                b"--TEST--\nbytes\n"
                b"--SKIPIF--\n<?php echo ''; ?>\n"
                b"--INI--\nprecision=14\n"
                b"--ENV--\nNAME=value\n"
                b"--ARGS--\n\"two words\" plain\n"
                b"--FILE--\n<?php echo \"ok\\x00\"; ?>\n"
                b"--CLEAN--\n<?php @unlink('out'); ?>\n"
                b"--EXPECT--\nok\x00\n"
            )
        )
        self.assertEqual(case.title, "bytes")
        self.assertEqual(case.expectation_mode, "EXPECT")
        self.assertEqual(case.source, b'<?php echo "ok\\x00"; ?>\n')
        self.assertEqual(case.sections["EXPECT"], b"ok\x00\n")

    def test_fileeof_strips_only_trailing_cr_and_lf(self) -> None:
        """Apply PHP's FILEEOF newline stripping without trimming other bytes."""
        case = RUNNER.parse_phpt(
            self.write_case(
                b"--TEST--\r\nfileeof\r\n"
                b"--EXPECTF--\r\nok%w\r\n"
                b"--FILEEOF--\r\n<?php echo 'ok'; ?> \r\n\r\n"
            )
        )
        self.assertEqual(case.source, b"<?php echo 'ok'; ?> ")
        self.assertEqual(case.expectation_mode, "EXPECTF")

    def test_external_source_and_expectation_are_resolved_beside_phpt(self) -> None:
        """Resolve locked FILE/EXPECT external fixtures without allowing traversal."""
        (self.root / "program.inc").write_bytes(b"<?php echo 'external'; ?>\n")
        (self.root / "expected.out").write_bytes(b"external\n")
        case = RUNNER.parse_phpt(
            self.write_case(
                b"--TEST--\nexternal\n"
                b"--FILE_EXTERNAL--\nprogram.inc\n"
                b"--EXPECT_EXTERNAL--\nexpected.out\n"
            )
        )
        self.assertEqual(case.source, b"<?php echo 'external'; ?>\n")
        self.assertEqual(case.expectation_mode, "EXPECT")
        self.assertEqual(case.sections["EXPECT"], b"external\n")

        with self.assertRaisesRegex(RUNNER.HarnessError, "unsafe FILE_EXTERNAL"):
            RUNNER.parse_phpt(
                self.write_case(
                    b"--TEST--\nunsafe\n"
                    b"--FILE_EXTERNAL--\n../program.inc\n"
                    b"--EXPECT--\nexternal\n"
                )
            )

    def test_done_sentinel_is_retained_and_ends_file_data(self) -> None:
        """Mirror run-tests.php's __halt_compiler payload sentinel behavior."""
        case = RUNNER.parse_phpt(
            self.write_case(
                b"--TEST--\ndone\n"
                b"--EXPECT--\nok\n"
                b"--FILE--\n<?php __halt_compiler(); ?>\n"
                b"===DONE===\nignored-data\n"
            )
        )
        self.assertEqual(
            case.source,
            b"<?php __halt_compiler(); ?>\n===DONE===\n",
        )

    def test_missing_or_multiple_required_sections_bork(self) -> None:
        """Reject absent FILE and multiple expectation modes deterministically."""
        with self.assertRaisesRegex(RUNNER.HarnessError, "FILE"):
            RUNNER.parse_phpt(
                self.write_case(b"--TEST--\nmissing\n--EXPECT--\nok\n")
            )
        with self.assertRaisesRegex(RUNNER.HarnessError, "exactly one --EXPECT"):
            RUNNER.parse_phpt(
                self.write_case(
                    b"--TEST--\nmultiple\n--FILE--\n<?php ?>\n"
                    b"--EXPECT--\na\n--EXPECTREGEX--\na\n"
                )
            )

    def test_unknown_and_known_unsupported_sections_bork_differently(self) -> None:
        """Distinguish malformed section names from valid non-CLI PHPT surfaces."""
        with self.assertRaisesRegex(RUNNER.HarnessError, "unknown section"):
            RUNNER.parse_phpt(
                self.write_case(
                    b"--TEST--\nunknown\n--FILE--\n<?php ?>\n"
                    b"--NOPE--\nvalue\n--EXPECT--\n\n"
                )
            )
        with self.assertRaisesRegex(RUNNER.HarnessError, "unsupported PHPT section.*STDIN"):
            RUNNER.parse_phpt(
                self.write_case(
                    b"--TEST--\nstdin\n--FILE--\n<?php ?>\n"
                    b"--STDIN--\nvalue\n--EXPECT--\n\n"
                )
            )


class SectionSemanticsTests(unittest.TestCase):
    """Verify INI, ENV, ARGS, SKIPIF, and output normalization behavior."""

    def test_ini_substitutions_and_assignment_rules(self) -> None:
        """Resolve PWD/TMP/ENV and ignore non-assignment lines like php-src."""
        settings = RUNNER.parse_ini(
            b"path={PWD}\ntmp={TMP}\nfrom_env={ENV:PHPT_TOKEN}\nignored\n spaced = yes \n",
            Path("/suite/case"),
            Path("/tmp/harness"),
            {"PHPT_TOKEN": "secret"},
        )
        self.assertEqual(
            settings,
            [
                ("path", "/suite/case"),
                ("tmp", "/tmp/harness"),
                ("from_env", "secret"),
                ("spaced", "yes"),
            ],
        )
        with self.assertRaisesRegex(RUNNER.HarnessError, "MISSING"):
            RUNNER.parse_ini(
                b"x={ENV:MISSING}\n", Path("/suite"), Path("/tmp"), {}
            )

    def test_environment_preserves_equals_and_substitutes_pwd(self) -> None:
        """Apply ENV entries after inheriting the base process environment."""
        environment = RUNNER.parse_environment(
            b"HERE={PWD}\nTOKEN=a=b=c\ninvalid\n",
            Path("/suite/case"),
            {"BASE": "kept"},
        )
        self.assertEqual(environment["BASE"], "kept")
        self.assertEqual(environment["HERE"], "/suite/case")
        self.assertEqual(environment["TOKEN"], "a=b=c")

    def test_isolated_environment_owns_all_temporary_variables(self) -> None:
        """Keep oracle and Elephc temporary side effects inside their own sandboxes."""
        with tempfile.TemporaryDirectory() as directory:
            temporary = Path(directory) / "private-tmp"
            environment = RUNNER.isolated_environment(
                b"TOKEN=works\n",
                Path("/suite/case"),
                temporary,
                {"TMPDIR": "/shared", "TMP": "/shared", "TEMP": "/shared"},
            )
            self.assertTrue(temporary.is_dir())
            self.assertEqual(environment["TMPDIR"], str(temporary))
            self.assertEqual(environment["TMP"], str(temporary))
            self.assertEqual(environment["TEMP"], str(temporary))
            self.assertEqual(environment["TOKEN"], "works")

    def test_raw_args_retain_shell_semantics(self) -> None:
        """Preserve quoting for the POSIX-shell execution path and reject bad syntax."""
        raw = RUNNER.validate_args(b'"two words" plain')
        self.assertEqual(raw, '"two words" plain')
        command = RUNNER.shell_execution_command(
            [sys.executable, "-c", "import sys;print('|'.join(sys.argv[1:]))"], raw
        )
        execution = RUNNER.run_process(
            command,
            cwd=Path.cwd(),
            environment=dict(os.environ),
            timeout_seconds=5,
        )
        self.assertEqual(execution.returncode, 0)
        self.assertEqual(execution.output, b"two words|plain\n")
        with self.assertRaisesRegex(RUNNER.HarnessError, "quoting"):
            RUNNER.validate_args(b"'unterminated")

    def test_skipif_classification_is_strict(self) -> None:
        """Accept php-src control prefixes while rejecting arbitrary output."""
        self.assertEqual(RUNNER.classify_skipif(b" skip reason\r\n"), ("skip", "reason"))
        self.assertEqual(RUNNER.classify_skipif(b"info detail\n"), ("run", "detail"))
        self.assertEqual(RUNNER.classify_skipif(b"warn detail\n"), ("run", "detail"))
        self.assertEqual(RUNNER.classify_skipif(b""), ("run", ""))
        self.assertEqual(RUNNER.classify_skipif(b"surprise\n"), ("invalid", "surprise"))

    def test_extensions_are_parsed_in_source_order(self) -> None:
        """Ignore blank EXTENSIONS lines without hiding required module names."""
        self.assertEqual(
            RUNNER.required_extensions(b"simplexml\n\nxsl\r\nzend_test\n"),
            ["simplexml", "xsl", "zend_test"],
        )

    def test_normalization_changes_crlf_but_not_lone_cr(self) -> None:
        """Match PHP trim semantics without incorrectly rewriting lone carriage returns."""
        self.assertEqual(
            RUNNER.normalize_output(b" \tvalue\r\nnext\r\x00\x0b"),
            b"value\nnext",
        )


class SandboxTests(unittest.TestCase):
    """Exercise fixture staging and deterministic filesystem delta reporting."""

    def setUp(self) -> None:
        """Create a synthetic php-src-shaped fixture tree."""
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.source_root = self.root / "php-src"
        tests = self.source_root / "ext/simplexml/tests"
        tests.mkdir(parents=True)
        self.phpt = tests / "case.phpt"
        self.phpt.write_bytes(
            b"--TEST--\nfixture\n--FILE--\n<?php echo 'ok'; ?>\n--EXPECT--\nok\n"
        )
        (tests / "book.xml").write_text("<book/>")
        external = self.source_root / "ext/xsl/tests/data.xml"
        external.parent.mkdir(parents=True)
        external.write_text("<external/>")
        subprocess.run(
            ["git", "init", "--quiet"], cwd=self.source_root, check=True
        )
        subprocess.run(
            ["git", "add", "."], cwd=self.source_root, check=True
        )
        subprocess.run(
            [
                "git",
                "-c",
                "user.name=PHPT Harness",
                "-c",
                "user.email=phpt@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "fixture",
            ],
            cwd=self.source_root,
            check=True,
        )

    def tearDown(self) -> None:
        """Remove the synthetic source and sandbox trees."""
        self.temporary.cleanup()

    def test_stage_sandbox_copies_internal_and_explicit_external_fixtures(self) -> None:
        """Preserve php-src-relative paths and extract FILE beside its PHPT."""
        case = RUNNER.parse_phpt(self.phpt)
        sandbox = RUNNER.stage_sandbox(
            self.source_root,
            "ext/simplexml/tests",
            "ext/simplexml/tests/case.phpt",
            case,
            self.root / "sandbox",
            ["ext/xsl/tests/data.xml"],
        )
        self.assertEqual(
            (sandbox.root / "ext/simplexml/tests/book.xml").read_text(), "<book/>"
        )
        self.assertEqual(
            (sandbox.root / "ext/xsl/tests/data.xml").read_text(), "<external/>"
        )
        self.assertEqual(sandbox.source_path.read_bytes(), case.source)

    def test_stage_elephc_native_project_copies_locked_pcre2_fixture(self) -> None:
        """Place the committed managed-PCRE2 manifest and lock at sandbox root."""
        repo_root = self.root / "repo"
        fixture = repo_root / RUNNER.ELEPHC_NATIVE_PROJECT_FIXTURE
        fixture.mkdir(parents=True)
        (fixture / "elephc.toml").write_text("manifest")
        (fixture / "elephc.lock").write_text("lock")
        sandbox = self.root / "elephc-sandbox"
        sandbox.mkdir()

        RUNNER.stage_elephc_native_project(repo_root, sandbox)

        self.assertEqual((sandbox / "elephc.toml").read_text(), "manifest")
        self.assertEqual((sandbox / "elephc.lock").read_text(), "lock")

    def test_stage_elephc_native_project_fails_closed_on_missing_lock(self) -> None:
        """Reject an incomplete native fixture instead of silently losing PCRE2."""
        repo_root = self.root / "repo"
        fixture = repo_root / RUNNER.ELEPHC_NATIVE_PROJECT_FIXTURE
        fixture.mkdir(parents=True)
        (fixture / "elephc.toml").write_text("manifest")
        sandbox = self.root / "elephc-sandbox"
        sandbox.mkdir()

        with self.assertRaisesRegex(RUNNER.HarnessError, "fixture is missing"):
            RUNNER.stage_elephc_native_project(repo_root, sandbox)

    def test_compiler_environment_restores_toolchain_fingerprint_inputs(self) -> None:
        """Do not create one managed-native cache key per isolated PHPT temp path."""
        runtime = {
            "PATH": "/test/bin",
            "TMPDIR": "/sandbox/tmp",
            "TMP": "/sandbox/tmp",
            "TEMP": "/sandbox/tmp",
            "SYSTEMROOT": "/test/system",
            "PHPT_VALUE": "kept",
        }
        base = {"PATH": "/host/bin", "TMPDIR": "/host/tmp"}

        environment = RUNNER.compiler_environment(runtime, base)

        self.assertEqual(environment["PATH"], "/host/bin")
        self.assertEqual(environment["TMPDIR"], "/host/tmp")
        self.assertNotIn("TMP", environment)
        self.assertNotIn("TEMP", environment)
        self.assertNotIn("SYSTEMROOT", environment)
        self.assertEqual(environment["PHPT_VALUE"], "kept")

    def test_snapshot_delta_records_add_modify_and_remove(self) -> None:
        """Report all three observable filesystem mutation kinds in stable order."""
        tree = self.root / "delta"
        tree.mkdir()
        (tree / "keep").write_text("same")
        (tree / "modify").write_text("before")
        (tree / "remove").write_text("gone")
        before = RUNNER.snapshot_tree(tree)
        (tree / "modify").write_text("after")
        (tree / "remove").unlink()
        (tree / "add").write_text("new")
        after = RUNNER.snapshot_tree(tree)
        delta = RUNNER.tree_delta(before, after)
        self.assertEqual(
            [(entry["path"], entry["change"]) for entry in delta],
            [("add", "added"), ("modify", "modified"), ("remove", "removed")],
        )


@unittest.skipUnless(shutil.which("php"), "a PHP CLI is required for harness integration")
class RunCaseIntegrationTests(unittest.TestCase):
    """Exercise the complete two-sandbox orchestration with a compiler stand-in."""

    def setUp(self) -> None:
        """Create a source tree and a compiler that links PHP-backed test executables."""
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.source_root = self.root / "php-src"
        tests = self.source_root / "ext/simplexml/tests"
        tests.mkdir(parents=True)
        self.entry = "ext/simplexml/tests/integration.phpt"
        (self.source_root / self.entry).write_bytes(
            b"--TEST--\nintegration\n"
            b"--ENV--\nTOKEN=works\n"
            b"--ARGS--\n\"two words\" plain\n"
            b"--SKIPIF--\n<?php ?>\n"
            b"--FILE--\n<?php\n"
            b"echo getenv('TOKEN'), '|', implode(',', array_slice($argv, 1));\n"
            b"file_put_contents(__DIR__ . '/out.tmp', 'same');\n"
            b"?>\n"
            b"--CLEAN--\n<?php @unlink(__DIR__ . '/out.tmp'); ?>\n"
            b"--EXPECT--\nworks|two words,plain\n"
        )
        self.php = Path(shutil.which("php") or "php").resolve()
        self.compiler = self.root / "fake-elephc"
        self.compiler.write_text(
            f"#!{sys.executable}\n"
            "import os\n"
            "import sys\n"
            "from pathlib import Path\n"
            "source = Path(sys.argv[-1]).resolve()\n"
            "binary = source.with_suffix('')\n"
            f"php = {str(self.php)!r}\n"
            "binary.write_text(\n"
            "    '#!' + sys.executable + '\\n'\n"
            "    + 'import os, sys\\n'\n"
            "    + f'os.execv({php!r}, [{php!r}, \\\"-n\\\", {str(source)!r}, *sys.argv[1:]])\\n'\n"
            ")\n"
            "binary.chmod(0o755)\n"
        )
        self.compiler.chmod(0o755)

    def tearDown(self) -> None:
        """Remove the synthetic compiler and php-src tree."""
        self.temporary.cleanup()

    def test_run_case_matches_output_exit_and_cleaned_file_deltas(self) -> None:
        """Reach passed only after control, output, side effect, and CLEAN parity."""
        repo_root = Path(__file__).resolve().parents[2]
        result = RUNNER.run_case(
            entry_path=self.entry,
            source_root=self.source_root,
            component_root_relative="ext/simplexml/tests",
            oracle=self.php,
            oracle_arguments=[],
            elephc=self.compiler,
            elephc_arguments=[],
            execution_prefix=[],
            target=None,
            repo_root=repo_root,
            matcher_path=repo_root / "tools/php-dom/phpt_match.php",
            extra_fixtures=[],
            timeout_seconds=5,
            keep_workspace=False,
        )
        self.assertEqual(result["status"], "passed", result["reason"])
        self.assertEqual(
            [(entry["path"], entry["change"]) for entry in result["oracle_file_delta"]],
            [("ext/simplexml/tests/out.tmp", "added")],
        )
        self.assertEqual(result["oracle_file_delta"], result["elephc_file_delta"])
        self.assertEqual(result["oracle_post_clean_delta"], [])
        self.assertEqual(result["elephc_post_clean_delta"], [])

    def test_run_case_executes_all_sections_from_the_sandbox_source_root(self) -> None:
        """Run SKIPIF, FILE, and CLEAN with php-src's TEST_PHP_SRCDIR CWD."""
        cwd_entry = "ext/simplexml/tests/source-root-cwd.phpt"
        (self.source_root / cwd_entry).write_bytes(
            b"--TEST--\nsource-root working directory\n"
            b"--SKIPIF--\n<?php\n"
            b"$root = dirname(__DIR__, 3);\n"
            b"if (getcwd() !== $root) echo 'skip wrong SKIPIF cwd';\n"
            b"?>\n"
            b"--FILE--\n<?php\n"
            b"$root = dirname(__DIR__, 3);\n"
            b"echo getcwd() === $root ? 'source-root' : 'wrong FILE cwd';\n"
            b"?>\n"
            b"--CLEAN--\n<?php\n"
            b"$root = dirname(__DIR__, 3);\n"
            b"if (getcwd() !== $root) echo 'wrong CLEAN cwd';\n"
            b"?>\n"
            b"--EXPECT--\nsource-root\n"
        )
        repo_root = Path(__file__).resolve().parents[2]

        result = RUNNER.run_case(
            entry_path=cwd_entry,
            source_root=self.source_root,
            component_root_relative="ext/simplexml/tests",
            oracle=self.php,
            oracle_arguments=[],
            elephc=self.compiler,
            elephc_arguments=[],
            execution_prefix=[],
            target=None,
            repo_root=repo_root,
            matcher_path=repo_root / "tools/php-dom/phpt_match.php",
            extra_fixtures=[],
            timeout_seconds=5,
            keep_workspace=False,
        )

        self.assertEqual(result["status"], "passed", result["reason"])
        self.assertEqual(result["oracle"]["skipif"]["returncode"], 0)
        self.assertEqual(result["elephc"]["skipif"]["returncode"], 0)
        self.assertEqual(result["oracle"]["clean"]["returncode"], 0)
        self.assertEqual(result["elephc"]["clean"]["returncode"], 0)

    def test_compile_failure_retains_complete_oracle_evidence(self) -> None:
        """Record PHP output, matching, and side effects even when Elephc cannot compile FILE."""
        failing_compiler = self.root / "failing-elephc"
        failing_compiler.write_text(f"#!{sys.executable}\nimport sys\nsys.exit(1)\n")
        failing_compiler.chmod(0o755)
        repo_root = Path(__file__).resolve().parents[2]
        result = RUNNER.run_case(
            entry_path=self.entry,
            source_root=self.source_root,
            component_root_relative="ext/simplexml/tests",
            oracle=self.php,
            oracle_arguments=[],
            elephc=failing_compiler,
            elephc_arguments=[],
            execution_prefix=[],
            target=None,
            repo_root=repo_root,
            matcher_path=repo_root / "tools/php-dom/phpt_match.php",
            extra_fixtures=[],
            timeout_seconds=5,
            keep_workspace=False,
        )
        self.assertEqual(result["status"], "failed")
        self.assertEqual(
            result["reason"],
            "Elephc SKIPIF did not compile",
        )
        self.assertEqual(result["oracle"]["skipif"]["returncode"], 0)

        no_skip_entry = "ext/simplexml/tests/compile-failure.phpt"
        (self.source_root / no_skip_entry).write_bytes(
            b"--TEST--\ncompile failure evidence\n"
            b"--FILE--\n<?php echo 'oracle'; file_put_contents(__DIR__ . '/evidence.tmp', 'yes'); ?>\n"
            b"--EXPECT--\noracle\n"
        )
        result = RUNNER.run_case(
            entry_path=no_skip_entry,
            source_root=self.source_root,
            component_root_relative="ext/simplexml/tests",
            oracle=self.php,
            oracle_arguments=[],
            elephc=failing_compiler,
            elephc_arguments=[],
            execution_prefix=[],
            target=None,
            repo_root=repo_root,
            matcher_path=repo_root / "tools/php-dom/phpt_match.php",
            extra_fixtures=[],
            timeout_seconds=5,
            keep_workspace=False,
        )
        self.assertEqual(result["status"], "failed")
        self.assertEqual(
            result["reason"],
            "Elephc FILE compilation failed or produced no executable",
        )
        self.assertEqual(result["oracle"]["file"]["output_base64"], "b3JhY2xl")
        self.assertEqual(result["oracle"]["file"]["returncode"], 0)
        self.assertEqual(result["oracle"]["match"]["returncode"], 0)
        self.assertEqual(
            [(entry["path"], entry["change"]) for entry in result["oracle_file_delta"]],
            [("ext/simplexml/tests/evidence.tmp", "added")],
        )
        self.assertEqual(result["elephc_file_delta"], [])


@unittest.skipUnless(shutil.which("php"), "a PHP CLI is required for PCRE matcher tests")
class PhpMatcherTests(unittest.TestCase):
    """Cross-check EXPECT modes through PHP's PCRE engine rather than Python re."""

    def setUp(self) -> None:
        """Create matcher inputs under one disposable directory."""
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.php = Path(shutil.which("php") or "php")
        self.matcher = Path(__file__).with_name("phpt_match.php")

    def tearDown(self) -> None:
        """Remove matcher input files."""
        self.temporary.cleanup()

    def match(self, mode: str, expected: bytes, actual: bytes) -> bool:
        """Invoke the production matcher and return its Boolean result."""
        matched, _ = RUNNER.match_output(
            self.php,
            [],
            self.matcher,
            mode,
            expected,
            actual,
            self.root,
            dict(os.environ),
            5,
        )
        return matched

    def test_expect_normalizes_crlf_and_php_trim_bytes(self) -> None:
        """Apply exact EXPECT normalization around binary-safe byte comparison."""
        self.assertTrue(self.match("EXPECT", b"value\n", b" value\r\n\x00"))
        self.assertFalse(self.match("EXPECT", b"value", b"value\rinside"))

    def test_expectf_supports_raw_regex_float_and_nul_tokens(self) -> None:
        """Exercise %r regions and placeholders missed by naive EXPECTF ports."""
        expected = b"word=%r[a-z]{3}%r int=%d float=%f nul=%0 end"
        actual = b"word=abc int=42 float=-.25E+2 nul=\x00 end"
        self.assertTrue(self.match("EXPECTF", expected, actual))
        self.assertFalse(self.match("EXPECTF", expected, b"word=123 int=x float=x nul=x"))

    def test_expectregex_accepts_pcre_branch_reset_groups(self) -> None:
        """Prove EXPECTREGEX is evaluated as PCRE, including non-Python syntax."""
        self.assertTrue(self.match("EXPECTREGEX", b"(?|foo|bar)", b"bar"))

    def test_required_builtin_extension_needs_no_shared_module(self) -> None:
        """Recognize an already-loaded EXTENSIONS dependency without adding INI."""
        settings, missing, executions = RUNNER.resolve_oracle_extensions(
            self.php,
            [],
            ["json"],
            dict(os.environ),
            5,
        )
        self.assertEqual(settings, [])
        self.assertEqual(missing, [])
        self.assertEqual(len(executions), 1)


if __name__ == "__main__":
    unittest.main()
