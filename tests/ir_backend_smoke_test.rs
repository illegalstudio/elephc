//! Purpose:
//! Integration smoke tests for the opt-in EIR backend CLI path.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - These tests exercise the binary-level `--ir-backend` path instead of only
//!   testing library helpers.

use std::fs;
use std::process::Command;

/// Returns the path to the cargo-built `elephc` binary.
fn elephc_cli_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Verifies the IR backend compiles, links, and runs straight-line scalar echo programs.
#[test]
fn ir_backend_echoes_scalar_literals() {
    for (name, source, expected) in [
        ("int", "<?php echo 42;", "42"),
        ("string", "<?php echo \"hi\";", "hi"),
        ("bool_true", "<?php echo true;", "1"),
        ("bool_false", "<?php echo false;", ""),
        ("null", "<?php echo null;", ""),
        ("float", "<?php echo 1.5;", "1.5"),
        ("local_store", "<?php $x = 40; echo $x;", "40"),
        ("argc_load", "<?php echo $argc;", "1"),
        ("iadd", "<?php echo $argc + 2;", "3"),
        ("isub", "<?php echo $argc - 1;", "0"),
        ("imul", "<?php echo $argc * 3;", "3"),
    ] {
        let output = compile_and_run_ir_backend(name, source);
        assert_eq!(output, expected, "unexpected stdout for {name}");
    }
}

/// Verifies integer comparisons and conditional branches on both branch directions.
#[test]
fn ir_backend_branches_on_integer_comparison() {
    let source = "<?php if ($argc > 1) { echo 9; } else { echo 4; }";
    assert_eq!(compile_and_run_ir_backend("if_false", source), "4");
    assert_eq!(
        compile_and_run_ir_backend_with_args("if_true", source, &["extra"]),
        "9"
    );
}

/// Verifies branch back-edges and repeated local slot updates in a while loop.
#[test]
fn ir_backend_runs_simple_while_loop() {
    let source = "<?php $i = 0; while ($i < 3) { echo $i; $i = $i + 1; }";
    assert_eq!(compile_and_run_ir_backend("while_loop", source), "012");
}

/// Verifies scalar EIR opcodes that are emitted for arithmetic, truthiness, and string coercion.
#[test]
fn ir_backend_handles_scalar_ops_and_string_coercions() {
    for (name, source, expected) in [
        ("idiv", "<?php echo 7 / 2;", "3.5"),
        ("imod", "<?php echo 7 % 4;", "3"),
        ("ineg", "<?php echo -$argc;", "-1"),
        ("bitwise", "<?php echo 6 & 3; echo 4 | 1; echo 7 ^ 3;", "254"),
        ("shifts", "<?php echo 1 << 3; echo -8 >> 1;", "8-4"),
        (
            "float_ops",
            "<?php echo 1.5 + 2.0; echo 5.0 / 2.0; echo -1.5;",
            "3.52.5-1.5",
        ),
        (
            "truthy_strings",
            "<?php if (\"0\") { echo 1; } else { echo 0; } if (\"hi\") { echo 2; }",
            "02",
        ),
        ("null_coalesce", "<?php $x = null; echo $x ?? 5;", "5"),
        ("concat_int", "<?php echo $argc . \"x\";", "1x"),
        ("concat_false", "<?php echo false . \"x\";", "x"),
        ("concat_null", "<?php echo null . \"x\";", "x"),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies scalar equality opcodes generated for loose comparisons, strict comparisons, and match.
#[test]
fn ir_backend_handles_scalar_equality() {
    for (name, source, expected) in [
        ("loose_int_eq", "<?php if ($argc == 1) { echo 1; }", "1"),
        ("loose_int_ne", "<?php if ($argc != 2) { echo 2; }", "2"),
        ("strict_int_eq", "<?php if (1 === 1) { echo 3; }", "3"),
        ("strict_int_ne", "<?php if (1 !== 2) { echo 4; }", "4"),
        ("strict_type_mismatch", "<?php if (1 !== true) { echo 5; }", "5"),
        ("loose_bool_truthy", "<?php if (($argc + 1) == true) { echo 6; }", "6"),
        ("strict_string_eq", "<?php if (\"a\" === \"a\") { echo 7; }", "7"),
        ("strict_string_ne", "<?php if (\"a\" !== \"b\") { echo 8; }", "8"),
        ("loose_string_eq", "<?php if (\"a\" == \"a\") { echo 9; }", "9"),
        ("loose_string_ne", "<?php if (\"a\" != \"b\") { echo 10; }", "10"),
        ("match_int", "<?php echo match ($argc) { 1 => 11, default => 0 };", "11"),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies print output and scalar switch dispatch through the EIR backend.
#[test]
fn ir_backend_handles_print_and_switch() {
    assert_eq!(
        compile_and_run_ir_backend("print_expr", "<?php print \"p\"; echo print \"q\";"),
        "pq1"
    );

    let switch_source = "<?php switch ($argc) { case 1: echo 1; break; case 2: echo 2; break; default: echo 9; }";
    assert_eq!(compile_and_run_ir_backend("switch_case_one", switch_source), "1");
    assert_eq!(
        compile_and_run_ir_backend_with_args("switch_case_two", switch_source, &["extra"]),
        "2"
    );
    assert_eq!(
        compile_and_run_ir_backend_with_args(
            "switch_default",
            switch_source,
            &["extra", "another"]
        ),
        "9"
    );
}

/// Verifies direct user-defined function calls with scalar params and returns.
#[test]
fn ir_backend_calls_user_functions() {
    for (name, source, expected) in [
        ("fn_return", "<?php function f() { return 42; } echo f();", "42"),
        (
            "fn_add",
            "<?php function add($a, $b) { return $a + $b; } echo add(2, 3);",
            "5",
        ),
        (
            "fn_void",
            "<?php function twice($x) { echo $x; echo $x; } twice(7);",
            "77",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies scalar builtin calls lowered by the EIR backend.
#[test]
fn ir_backend_handles_scalar_builtins() {
    for (name, source, expected) in [
        ("strlen", "<?php echo strlen(\"hello\");", "5"),
        ("intval_float", "<?php echo intval(3.9);", "3"),
        ("intval_str", "<?php echo intval(\"42xyz\");", "42"),
        ("floatval_int", "<?php echo floatval(2) + 0.5;", "2.5"),
        ("floatval_str", "<?php echo floatval(\"2.5x\");", "2.5"),
        ("boolval_false", "<?php echo boolval(\"0\");", ""),
        ("boolval_true", "<?php echo boolval(\"hi\");", "1"),
        (
            "type_predicates",
            "<?php echo is_int(1); echo is_float(1.5); echo is_bool(false); echo is_null(null); echo is_string(\"x\");",
            "11111",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Compiles `source` with `--ir-backend`, runs the output binary, and returns stdout.
fn compile_and_run_ir_backend(name: &str, source: &str) -> String {
    compile_and_run_ir_backend_with_args(name, source, &[])
}

/// Compiles `source`, runs the output binary with extra args, and returns stdout.
fn compile_and_run_ir_backend_with_args(name: &str, source: &str, args: &[&str]) -> String {
    let dir = std::env::temp_dir().join(format!(
        "elephc_ir_backend_{}_{}_{}",
        name,
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&dir).expect("failed to create IR backend hello directory");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("failed to write IR backend PHP fixture");

    let compile = Command::new(elephc_cli_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir)
        .arg("--ir-backend")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --ir-backend");
    assert!(
        compile.status.success(),
        "elephc --ir-backend failed for {name}: stderr={}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(dir.join("main"))
        .current_dir(&dir)
        .args(args)
        .output()
        .expect("failed to run IR backend binary");
    assert!(run.status.success(), "IR backend binary failed for {name}");
    let stdout = String::from_utf8(run.stdout).unwrap();

    let _ = fs::remove_dir_all(&dir);
    stdout
}

/// Returns a coarse unique suffix for temporary test directories.
fn unique_test_id() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos()
}
