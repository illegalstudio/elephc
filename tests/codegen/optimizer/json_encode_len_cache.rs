//! Purpose:
//! End-to-end coverage for the loop-invariant `strlen(json_encode($rows))` length cache.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Scalar rows take the preheader guard and return one cached length; object, nested or
//!   unknown row values keep the per-iteration encoder and preserve `JsonSerializable` side
//!   effects.
//! - Another JSON call in the loop, a mutated or aliased source, dynamic flags and a
//!   non-positive trip count must keep the generic per-iteration encode.
//! - Assertions cover EIR probe/dispatch shape, output parity, `--ir-opt` parity, heap
//!   cleanliness and per-target compilation.

use crate::support::*;

/// Emits the optimized or unoptimized EIR module text of one fixture.
fn emit_ir(source: &str, ir_opt: bool) -> String {
    let dir = make_cli_test_dir("elephc_json_len_cache_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write json length cache fixture");
    let mut command = elephc_cli_command(&dir);
    command.arg("--emit-ir");
    if !ir_opt {
        command.arg("--no-ir-opt");
    }
    let output = command.arg(&path).output().expect("run elephc --emit-ir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(dir);
    String::from_utf8(output.stdout).expect("EIR is UTF-8")
}

/// Returns the `main` function body from one EIR module dump.
fn main_ir(source: &str) -> String {
    emit_ir(source, true)
        .split("function main(")
        .nth(1)
        .expect("EIR contains main")
        .split("\n  function ")
        .next()
        .expect("main body")
        .to_string()
}

/// Compiles and runs one fixture with an explicit EIR optimizer mode.
fn run_with_ir_opt(source: &str, mode: &str) -> String {
    let dir = make_cli_test_dir("elephc_json_len_cache_run");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write json length cache run fixture");
    let compiled = elephc_cli_command(&dir)
        .arg(mode)
        .arg(&path)
        .output()
        .expect("compile json length cache fixture");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = std::process::Command::new(dir.join("main"))
        .output()
        .expect("run json length cache fixture");
    let _ = fs::remove_dir_all(dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("json length cache output is UTF-8")
}

/// Scalar rows cache the encoded length in the loop preheader.
#[test]
fn test_json_encode_len_cache_scalar_rows() {
    let source = r#"<?php
$rows = [];
for ($i = 0; $i < 100; $i++) {
    $rows[] = ["id" => $i, "name" => "item" . $i, "active" => true];
}
$n = 0;
for ($i = 0; $i < 5 + $argc; $i++) {
    $n += strlen(json_encode($rows));
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "23886\n");
    let main = main_ir(source);
    assert!(main.contains("json_rows_scalar_guard"), "{main}");
    assert!(main.contains("json_encode_len_cached"), "{main}");
    assert!(main.contains("json_len_probe"), "{main}");
}

/// Scalar int/string/bool/null/float row values all take the cached path.
#[test]
fn test_json_encode_len_cache_scalar_value_kinds() {
    let source = r#"<?php
$rows = [["i" => 1, "s" => "x", "b" => true, "n" => null, "f" => 1.5]];
$n = 0;
for ($i = 0; $i < 5 + $argc; $i++) {
    $n += strlen(json_encode($rows));
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "258\n");
    let main = main_ir(source);
    assert!(main.contains("json_encode_len_cached"), "{main}");
}

/// A row object makes the guard fall back per iteration and keeps JsonSerializable calls.
#[test]
fn test_json_encode_len_cache_object_rows_fall_back_at_runtime() {
    let source = r#"<?php
class Row implements JsonSerializable {
    public static int $calls = 0;
    public function jsonSerialize(): mixed { self::$calls++; return ["id" => 1]; }
}
$rows = [["id" => 1, "obj" => new Row()]];
$n = 0;
for ($i = 0; $i < 5 + $argc; $i++) { $n += strlen(json_encode($rows)); }
echo $n, ":", Row::$calls, "\n";
"#;
    assert_eq!(compile_and_run(source), "150:6\n");
    let main = main_ir(source);
    assert!(main.contains("json_encode_len_cached"), "{main}");
}

/// A nested row value makes the guard fall back per iteration.
#[test]
fn test_json_encode_len_cache_nested_rows_fall_back_at_runtime() {
    let source = r#"<?php
$rows = [["id" => [1, 2]]];
$n = 0;
for ($i = 0; $i < 5 + $argc; $i++) { $n += strlen(json_encode($rows)); }
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "84\n");
    let main = main_ir(source);
    assert!(main.contains("json_encode_len_cached"), "{main}");
}

/// A NaN row value still caches the deterministic empty/error length.
#[test]
fn test_json_encode_len_cache_nan_value_keeps_php_length() {
    let source = r#"<?php
$rows = [["f" => NAN]];
$n = 0;
for ($i = 0; $i < 5 + $argc; $i++) { $n += strlen(json_encode($rows)); }
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "0\n");
    let main = main_ir(source);
    assert!(main.contains("json_encode_len_cached"), "{main}");
}

/// Another JSON call in the loop keeps the per-iteration encoder.
#[test]
fn test_json_encode_len_cache_other_json_state_call_falls_back() {
    let source = r#"<?php
$rows = [["id" => 1]];
$n = 0;
for ($i = 0; $i < 5 + $argc; $i++) {
    $n += strlen(json_encode($rows));
    json_decode("{bad");
    $n += json_last_error();
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "84\n");
    let main = main_ir(source);
    assert!(!main.contains("json_encode_len_cached"), "{main}");
}

/// Mutating the rows inside the loop keeps the per-iteration encoder.
#[test]
fn test_json_encode_len_cache_mutated_rows_fall_back() {
    let source = r#"<?php
$rows = [["id" => 1]];
$n = 0;
for ($i = 0; $i < 5 + $argc; $i++) {
    $n += strlen(json_encode($rows));
    $rows[0]["id"] = $i;
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "60\n");
    let main = main_ir(source);
    assert!(!main.contains("json_encode_len_cached"), "{main}");
}

/// Aliasing the rows by reference keeps the per-iteration encoder.
#[test]
fn test_json_encode_len_cache_reference_alias_falls_back() {
    let source = r#"<?php
$rows = [["id" => 1]];
$ref = &$rows;
$n = 0;
for ($i = 0; $i < 5 + $argc; $i++) {
    $n += strlen(json_encode($rows));
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "60\n");
    let main = main_ir(source);
    assert!(!main.contains("json_encode_len_cached"), "{main}");
}

/// Explicit flags keep the lowering's flag-aware encoder and no cache.
#[test]
fn test_json_encode_len_cache_flags_fall_back() {
    let source = r#"<?php
$rows = [["id" => 1, "name" => "x"]];
$n = 0;
for ($i = 0; $i < 5 + $argc; $i++) {
    $n += strlen(json_encode($rows, JSON_PRETTY_PRINT));
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "312\n");
    let main = main_ir(source);
    assert!(!main.contains("json_encode_len_cached"), "{main}");
}

/// A zero-trip loop never runs the probe or the encoder.
#[test]
fn test_json_encode_len_cache_zero_trip_loop_skips_probe() {
    let source = r#"<?php
class Row implements JsonSerializable {
    public static int $calls = 0;
    public function jsonSerialize(): mixed { self::$calls++; return ["id" => 1]; }
}
$rows = [["obj" => new Row()]];
$n = 0;
for ($i = 0; $i < $argc - 5; $i++) { $n += strlen(json_encode($rows)); }
echo $n, ":", Row::$calls, "\n";
"#;
    assert_eq!(compile_and_run(source), "0:0\n");
    let main = main_ir(source);
    assert!(!main.contains("json_encode_len_cached"), "{main}");
}

/// An empty literal does not match the associative-row shape and stays generic.
#[test]
fn test_json_encode_len_cache_empty_rows_fall_back() {
    let source = r#"<?php
$rows = [];
$n = 0;
for ($i = 0; $i < 5 + $argc; $i++) { $n += strlen(json_encode($rows)); }
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "12\n");
    let main = main_ir(source);
    assert!(!main.contains("json_encode_len_cached"), "{main}");
}

/// The cached and generic paths agree with the EIR optimizer on and off.
#[test]
fn test_json_encode_len_cache_ir_opt_parity() {
    let source = r#"<?php
$rows = [];
for ($i = 0; $i < 100; $i++) {
    $rows[] = ["id" => $i, "name" => "item" . $i, "active" => true];
}
$n = 0;
for ($i = 0; $i < 5 + $argc; $i++) {
    $n += strlen(json_encode($rows));
}
echo $n, "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "23886\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "23886\n");
    assert!(
        emit_ir(source, true).contains("json_encode_len_cached"),
        "optimized EIR must contain the cached dispatch"
    );
    assert!(
        !emit_ir(source, false).contains("json_encode_len_cached"),
        "unoptimized EIR must keep the generic encoder"
    );
}

/// The cached length and its one owned probe string stay heap-clean.
#[test]
fn test_json_encode_len_cache_heap_debug_is_clean() {
    let source = r#"<?php
$rows = [];
for ($i = 0; $i < 100; $i++) {
    $rows[] = ["id" => $i, "name" => "item" . $i, "active" => true];
}
$n = 0;
for ($i = 0; $i < 5 + $argc; $i++) {
    $n += strlen(json_encode($rows));
}
echo $n, "\n";
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "23886\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Every supported target compiles the probe and cached dispatch.
#[test]
fn test_json_encode_len_cache_emits_all_supported_targets() {
    let source = r#"<?php
$rows = [];
for ($i = 0; $i < 100; $i++) {
    $rows[] = ["id" => $i, "name" => "item" . $i, "active" => true];
}
$n = 0;
for ($i = 0; $i < 5 + $argc; $i++) {
    $n += strlen(json_encode($rows));
}
echo $n, "\n";
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let dir = make_cli_test_dir("elephc_json_len_cache_targets");
        let path = dir.join("main.php");
        fs::write(&path, source).expect("write json length cache target fixture");
        let output = elephc_cli_command(&dir)
            .args(["--emit", "staticlib", "--emit-asm", "--target", target])
            .arg(&path)
            .output()
            .expect("run elephc --emit-asm");
        assert!(
            output.status.success(),
            "emit-asm for {target} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let assembly = fs::read_to_string(path.with_extension("s"))
            .expect("read json length cache target assembly");
        assert!(!assembly.is_empty(), "{target}: empty assembly");
        let _ = fs::remove_dir_all(dir);
    }
}
