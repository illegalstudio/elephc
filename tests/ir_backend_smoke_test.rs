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
use std::path::Path;
use std::process::{Command, Output};

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
        ("error_suppress_expr", "<?php echo @(\"ok\");", "ok"),
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
        (
            "fn_stack_int_arg",
            "<?php function pick($a, $b, $c, $d, $e, $f, $g, $h, $i) { echo $i; } pick(1, 2, 3, 4, 5, 6, 7, 8, 9);",
            "9",
        ),
        (
            "fn_stack_string_arg",
            "<?php function tail($a, $b, $c, $d, $e, $f, $g, $s) { echo $s; } tail(1, 2, 3, 4, 5, 6, 7, \"tail\");",
            "tail",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies fatal terminators emitted for implicit `never` returns write the legacy diagnostic.
#[test]
fn ir_backend_handles_fatal_never_implicit_return() {
    let run = compile_ir_backend_and_run(
        "fatal_never_implicit_return",
        "<?php function fail(): never { } fail(); echo \"unreachable\";",
        &[],
    );
    assert!(
        !run.status.success(),
        "IR backend fatal fixture unexpectedly succeeded"
    );
    assert_eq!(
        String::from_utf8(run.stdout).expect("fatal stdout should be utf8"),
        ""
    );
    let stderr = String::from_utf8(run.stderr).expect("fatal stderr should be utf8");
    assert!(
        stderr.contains("Fatal error: A never-returning function must not implicitly return"),
        "unexpected fatal stderr: {stderr}"
    );
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

/// Verifies scalar casts and string indexing lowered by the EIR backend.
#[test]
fn ir_backend_handles_scalar_casts_and_string_indexing() {
    for (name, source, expected) in [
        (
            "string_casts_to_numbers",
            "<?php echo (int)\"42xyz\"; echo \":\"; echo (float)\"2.5x\";",
            "42:2.5",
        ),
        (
            "scalar_casts_to_string",
            "<?php echo (string)7; echo \":\"; echo (string)1.5; echo \":\"; echo (string)false;",
            "7:1.5:",
        ),
        (
            "scalar_casts_to_bool",
            "<?php echo (bool)\"0\"; echo \":\"; echo (bool)\"hi\";",
            ":1",
        ),
        (
            "string_indexing",
            "<?php echo \"hello\"[1]; echo \":\"; echo \"hello\"[-1]; echo \":\"; echo \"hi\"[9];",
            "e:o:",
        ),
        (
            "string_switch_subject",
            "<?php switch (\"2\") { case 2: echo \"hit\"; }",
            "hit",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies dynamic scalar power and spaceship operators lowered by the EIR backend.
#[test]
fn ir_backend_handles_power_and_spaceship() {
    let source = "<?php echo $argc ** 3; echo \":\"; echo ($argc + 0.5) ** 2.0; echo \":\"; echo $argc <=> 2; echo \":\"; echo 2 <=> $argc;";
    assert_eq!(compile_and_run_ir_backend("pow_spaceship_argc_one", source), "1:2.25:-1:1");
    assert_eq!(
        compile_and_run_ir_backend_with_args("pow_spaceship_argc_two", source, &["extra"]),
        "8:6.25:0:0"
    );
}

/// Verifies explicit ownership ops emitted around string local slots.
#[test]
fn ir_backend_handles_string_ownership_ops() {
    for (name, source, expected) in [
        ("literal_string_acquire", "<?php $s = \"hello\"; echo $s;", "hello"),
        ("concat_string_acquire", "<?php $x = \"a\" . $argc; echo $x;", "a1"),
        (
            "string_copy_survives_source_release",
            "<?php $x = \"a\" . $argc; $y = $x; $x = \"b\" . $argc; echo $y;",
            "a1",
        ),
        (
            "string_release_on_overwrite",
            "<?php $x = \"a\" . $argc; $x = \"b\" . $argc; echo $x;",
            "b1",
        ),
        ("empty_string_release", "<?php $x = (string)false; $x = \"z\"; echo $x;", "z"),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies basic indexed-array allocation, append growth, and count lowering.
#[test]
fn ir_backend_handles_basic_indexed_arrays() {
    for (name, source, expected) in [
        ("array_count_ints", "<?php $a = [1, 2, 3]; echo count($a);", "3"),
        ("array_get_int", "<?php $a = [10, 20]; echo $a[1];", "20"),
        ("array_get_float", "<?php $a = [1.5, 2.5]; echo $a[1];", "2.5"),
        ("array_get_string", "<?php $a = [\"a\", \"b\"]; echo $a[1];", "b"),
        ("array_get_oob_null", "<?php $a = [10]; echo $a[9];", ""),
        ("array_get_negative_null", "<?php $a = [10]; echo $a[-1];", ""),
        ("array_count_strings", "<?php $a = [\"a\", \"b\"]; echo count($a);", "2"),
        (
            "array_push_grows_local",
            "<?php $a = []; $a[] = 1; $a[] = 2; $a[] = 3; $a[] = 4; $a[] = 5; echo count($a);",
            "5",
        ),
        ("array_set_int", "<?php $a = [10, 20]; $a[1] = 99; echo $a[1];", "99"),
        ("array_set_float", "<?php $a = [1.5, 2.5]; $a[0] = 3.5; echo $a[0];", "3.5"),
        ("array_set_string", "<?php $a = [\"a\", \"b\"]; $a[1] = \"z\"; echo $a[1];", "z"),
        ("array_set_extends_int", "<?php $a = [1]; $a[3] = 9; echo count($a); echo \":\"; echo $a[0];", "4:1"),
        ("array_set_extends_string", "<?php $a = [\"a\"]; $a[2] = \"z\"; echo count($a); echo \":\"; echo $a[2];", "3:z"),
        ("array_set_empty_count", "<?php $a = []; $a[2] = 7; echo count($a);", "3"),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }

    let dynamic_source = "<?php $a = [10, 20, 30]; echo $a[$argc];";
    assert_eq!(compile_and_run_ir_backend("array_get_dynamic_one", dynamic_source), "20");
    assert_eq!(
        compile_and_run_ir_backend_with_args("array_get_dynamic_two", dynamic_source, &["extra"]),
        "30"
    );
}

/// Verifies basic associative-array allocation, lookup, update, and count lowering.
#[test]
fn ir_backend_handles_basic_associative_arrays() {
    for (name, source, expected) in [
        ("hash_count", "<?php $h = [\"a\" => 1, \"b\" => 2]; echo count($h);", "2"),
        ("hash_get_int", "<?php $h = [\"a\" => 1]; echo $h[\"a\"];", "1"),
        ("hash_get_string", "<?php $h = [\"a\" => \"z\"]; echo $h[\"a\"];", "z"),
        ("hash_get_float", "<?php $h = [\"a\" => 1.5]; echo $h[\"a\"];", "1.5"),
        ("hash_get_miss", "<?php $h = [\"a\" => 1]; echo $h[\"missing\"];", ""),
        ("hash_int_key", "<?php $h = [1 => \"one\"]; echo $h[1];", "one"),
        ("hash_set_updates_local", "<?php $h = [\"a\" => 1]; $h[\"a\"] = 7; echo $h[\"a\"];", "7"),
        ("hash_set_string_value", "<?php $h = [\"a\" => \"x\"]; $h[\"a\"] = \"y\"; echo $h[\"a\"];", "y"),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies include-once guard lowering skips an already loaded include body.
#[test]
fn ir_backend_handles_include_once_guard() {
    let out = compile_and_run_ir_backend_files(
        "include_once_guard",
        &[
            (
                "main.php",
                "<?php include_once 'piece.php'; include_once 'piece.php';",
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
        &[],
    );
    assert_eq!(out, "piece");
}

/// Compiles `source` with `--ir-backend`, runs the output binary, and returns stdout.
fn compile_and_run_ir_backend(name: &str, source: &str) -> String {
    compile_and_run_ir_backend_with_args(name, source, &[])
}

/// Compiles `source`, runs the output binary with extra args, and returns stdout.
fn compile_and_run_ir_backend_with_args(name: &str, source: &str, args: &[&str]) -> String {
    let run = compile_ir_backend_and_run(name, source, args);
    assert!(run.status.success(), "IR backend binary failed for {name}");
    String::from_utf8(run.stdout).unwrap()
}

/// Compiles `source` with `--ir-backend`, runs the binary, and returns raw process output.
fn compile_ir_backend_and_run(name: &str, source: &str, args: &[&str]) -> Output {
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

    let _ = fs::remove_dir_all(&dir);
    run
}

/// Compiles multiple PHP files with `--ir-backend`, runs the entry binary, and returns stdout.
fn compile_and_run_ir_backend_files(
    name: &str,
    files: &[(&str, &str)],
    entry: &str,
    args: &[&str],
) -> String {
    let run = compile_ir_backend_files_and_run(name, files, entry, args);
    assert!(run.status.success(), "IR backend binary failed for {name}");
    String::from_utf8(run.stdout).unwrap()
}

/// Compiles a multi-file `--ir-backend` fixture and returns raw process output.
fn compile_ir_backend_files_and_run(
    name: &str,
    files: &[(&str, &str)],
    entry: &str,
    args: &[&str],
) -> Output {
    let dir = std::env::temp_dir().join(format!(
        "elephc_ir_backend_{}_{}_{}",
        name,
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&dir).expect("failed to create IR backend files directory");
    for (path, contents) in files {
        let path = dir.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("failed to create IR backend fixture parent");
        }
        fs::write(path, contents).expect("failed to write IR backend PHP fixture");
    }
    let entry_path = dir.join(entry);

    let compile = Command::new(elephc_cli_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir)
        .arg("--ir-backend")
        .arg(&entry_path)
        .output()
        .expect("failed to run elephc CLI with --ir-backend");
    assert!(
        compile.status.success(),
        "elephc --ir-backend failed for {name}: stderr={}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let binary_path = entry_binary_path(&entry_path);
    let run = Command::new(binary_path)
        .current_dir(&dir)
        .args(args)
        .output()
        .expect("failed to run IR backend binary");

    let _ = fs::remove_dir_all(&dir);
    run
}

/// Returns the binary path produced next to a PHP entry file.
fn entry_binary_path(entry_path: &Path) -> std::path::PathBuf {
    entry_path.with_extension("")
}

/// Returns a coarse unique suffix for temporary test directories.
fn unique_test_id() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos()
}
