//! Purpose:
//! End-to-end CLI tests for `--strict-php`: plain PHP compiles and runs
//! identically, user code may shadow extension builtin names, `function_exists`
//! reports extensions as missing, extension constructs fail with the strict
//! diagnostics, and injected preludes keep working.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every test spawns the real `elephc` binary (CARGO_BIN_EXE_elephc) so the
//!   whole pipeline — CLI parsing, thread-local strict state, audit hooks,
//!   catalog filtering, codegen — is exercised together.

use crate::support::*;

/// Compiles `source` through the CLI with `--strict-php`, runs the binary, and
/// returns its stdout. Panics if compilation or the run fails.
fn compile_strict_cli_and_run(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_cli_strict");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).unwrap();

    let compile_out = elephc_cli_command(&dir)
        .arg("--strict-php")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI");
    assert!(
        compile_out.status.success(),
        "elephc --strict-php failed: {}",
        String::from_utf8_lossy(&compile_out.stderr)
    );

    let bin_path = dir.join("main");
    let output = run_binary(&bin_path, &dir);
    assert!(
        output.status.success(),
        "strict-compiled binary exited with error"
    );

    let _ = fs::remove_dir_all(&dir);
    String::from_utf8(output.stdout).unwrap()
}

/// Compiles `source` through the CLI with the given extra flags, expecting the
/// compiler to fail; returns its stderr.
fn compile_cli_expect_error(source: &str, flags: &[&str]) -> String {
    let dir = make_cli_test_dir("elephc_cli_strict_err");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).unwrap();

    let compile_out = elephc_cli_command(&dir)
        .args(flags)
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI");
    assert!(
        !compile_out.status.success(),
        "expected elephc to fail with flags {flags:?}, but it succeeded"
    );

    let _ = fs::remove_dir_all(&dir);
    String::from_utf8_lossy(&compile_out.stderr).into_owned()
}

/// Verifies a plain PHP program produces the same output with and without
/// `--strict-php`: strict mode must not change the semantics of PHP code.
#[test]
fn test_strict_php_plain_program_runs_identically() {
    let source = r#"<?php
function fizzbuzz(int $n): string {
    if ($n % 15 == 0) { return "FizzBuzz"; }
    if ($n % 3 == 0) { return "Fizz"; }
    if ($n % 5 == 0) { return "Buzz"; }
    return strval($n);
}
for ($i = 1; $i <= 15; $i++) {
    echo fizzbuzz($i), "\n";
}
"#;
    let strict_out = compile_strict_cli_and_run(source);
    let default_out = compile_cli_file_and_run(source, &[]);
    assert_eq!(strict_out, default_out);
    assert!(strict_out.contains("FizzBuzz"));
}

/// Verifies user code may declare and call a function named after an extension
/// builtin under strict mode — the name does not exist in PHP, so this is plain
/// userland code and the call must dispatch to the user function.
#[test]
fn test_strict_php_user_declared_ptr_get_runs() {
    let out = compile_strict_cli_and_run(
        r#"<?php
function ptr_get(int $x): int { return $x + 1; }
echo ptr_get(41);
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies `function_exists()` reports extension builtins as missing under
/// strict mode (matching the PHP interpreter) while user-shadowed names exist.
#[test]
fn test_strict_php_function_exists_hides_extensions() {
    let out = compile_strict_cli_and_run(
        r#"<?php
function ptr_get(int $x): int { return $x; }
echo var_export(function_exists('ptr_get'), true), "\n";
echo var_export(function_exists('zval_pack'), true), "\n";
echo var_export(function_exists('buffer_new'), true), "\n";
echo var_export(function_exists('strlen'), true), "\n";
"#,
    );
    assert_eq!(out, "true\nfalse\nfalse\ntrue\n");
}

/// Verifies the same `function_exists()` probes report the extension builtins
/// as present without the flag, so strict mode is what makes the difference.
#[test]
fn test_default_mode_function_exists_keeps_extensions() {
    let out = compile_cli_file_and_run(
        r#"<?php
echo var_export(function_exists('zval_pack'), true), "\n";
echo var_export(function_exists('buffer_new'), true), "\n";
"#,
        &[],
    );
    assert_eq!(out, "true\ntrue\n");
}

/// Verifies a call to an extension builtin fails under strict mode with the
/// undefined-function diagnostic carrying the disabled-extension hint.
#[test]
fn test_strict_php_extension_call_fails_with_hint() {
    let stderr = compile_cli_expect_error(
        "<?php $p = ptr_null();",
        &["--strict-php"],
    );
    assert!(
        stderr.contains("Undefined function: ptr_null")
            && stderr.contains("disabled by --strict-php"),
        "unexpected stderr: {stderr}",
    );
}

/// Verifies extension syntax fails under strict mode with the audit diagnostic.
#[test]
fn test_strict_php_extension_syntax_fails() {
    let stderr = compile_cli_expect_error(
        "<?php packed class P { public int $x; }",
        &["--strict-php"],
    );
    assert!(
        stderr.contains("`packed class` is an elephc extension"),
        "unexpected stderr: {stderr}",
    );
}

/// Verifies `--strict-php` cannot be combined with `--define`, since defines
/// only feed the `ifdef` extension that strict mode rejects.
#[test]
fn test_strict_php_define_conflict_is_cli_error() {
    let stderr = compile_cli_expect_error(
        "<?php echo 1;",
        &["--strict-php", "--define", "FEATURE"],
    );
    assert!(
        stderr.contains("--strict-php cannot be combined with --define"),
        "unexpected stderr: {stderr}",
    );
}

/// Verifies a program that triggers a compiler prelude injection (var_export's
/// elephc-PHP prelude) still compiles and runs under strict mode: injected
/// compiler code is exempt from the audit and uses internal builtin aliases.
#[test]
fn test_strict_php_prelude_program_runs() {
    let out = compile_strict_cli_and_run(
        r#"<?php
echo var_export(true, true), "\n";
var_export([1, 2]);
"#,
    );
    assert!(
        out.starts_with("true\n") && out.contains("0 => 1"),
        "unexpected output: {out}",
    );
}

/// Verifies `--check --strict-php` reports strict violations without emitting
/// artifacts, so strict mode composes with check-only runs.
#[test]
fn test_strict_php_composes_with_check_mode() {
    let stderr = compile_cli_expect_error(
        "<?php int $x = 5;",
        &["--check", "--strict-php"],
    );
    assert!(
        stderr.contains("typed local variable declarations are an elephc extension"),
        "unexpected stderr: {stderr}",
    );
}
