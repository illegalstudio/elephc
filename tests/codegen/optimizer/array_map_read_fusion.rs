//! Purpose:
//! Regression coverage for fusing a pure `array_map()` temporary with its constant-indexed reads.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The fused IR must expose `array_isset` reads and drop `runtime.array_map`.
//! - Missing keys, source writes, typed/capturing/effectful callbacks, and uses across a
//!   control-flow block must keep the generic map.

use crate::support::*;

/// Emits the optimized `main` function body for one fixture.
fn main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_map_read_fusion_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write map-read fixture");
    let output = elephc_cli_command(&dir)
        .arg("--emit-ir")
        .arg(&path)
        .output()
        .expect("run elephc --emit-ir");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).expect("EIR is UTF-8");
    let main = text
        .split("function main(")
        .nth(1)
        .expect("EIR contains main")
        .split("\n  function ")
        .next()
        .expect("main body")
        .to_string();
    let _ = fs::remove_dir_all(dir);
    main
}

/// A pure closure's constant reads use guarded source reads instead of materializing the map.
#[test]
fn test_map_read_pure_closure_fuses_constant_reads() {
    let source = r#"<?php
$values = range(1, 10);
$mapped = array_map(fn($v) => $v * 3, $values);
echo $mapped[2], '|', $mapped[9];
"#;
    assert_eq!(compile_and_run(source), "9|30");
    let main = main_ir(source);
    assert!(!main.contains("runtime.array_map"), "{main}");
    assert_eq!(main.matches("array_isset").count(), 2, "{main}");
}

/// A compound pure arithmetic closure still fuses.
#[test]
fn test_map_read_compound_arithmetic_fuses() {
    let source = r#"<?php
$values = range(1, 10);
$mapped = array_map(fn($v) => ($v + 1) * 3 - 1, $values);
echo $mapped[2], '|', $mapped[9];
"#;
    assert_eq!(compile_and_run(source), "11|32");
    let main = main_ir(source);
    assert!(!main.contains("runtime.array_map"), "{main}");
}

/// Overflowing arithmetic keeps PHP's float promotion.
#[test]
fn test_map_read_overflow_keeps_php_float() {
    let source = r#"<?php
$values = [PHP_INT_MAX];
$mapped = array_map(fn($v) => $v * 2, $values);
echo gettype($mapped[0]), '|', $mapped[0];
"#;
    assert_eq!(compile_and_run(source), "double|1.844674407371E+19");
    let main = main_ir(source);
    assert!(!main.contains("runtime.array_map"), "{main}");
}

/// A missing source key keeps PHP's warning and null result.
#[test]
fn test_map_read_missing_key_warns_and_returns_null() {
    let source = r#"<?php
$values = [1, 2];
$mapped = array_map(fn($v) => $v * 2, $values);
var_dump($mapped[5]);
"#;
    let out = compile_and_run_capture(source);
    assert_eq!(out.stdout, "NULL\n");
    assert!(out.stderr.contains("Undefined array key 5"), "{}", out.stderr);
}

/// Writing the source after the map keeps the generic map.
#[test]
fn test_map_read_source_write_keeps_generic_map() {
    let source = r#"<?php
$values = [1, 2];
$mapped = array_map(fn($v) => $v * 2, $values);
$values[0] = 9;
echo $mapped[1];
"#;
    assert_eq!(compile_and_run(source), "4");
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map").count(), 1, "{main}");
}

/// A declared int return keeps the map so an overflowing element still throws.
#[test]
fn test_map_read_typed_callback_keeps_generic_map() {
    let source = r#"<?php
$values = [1, 2];
$mapped = array_map(fn($v): int => $v * 2, $values);
echo $mapped[1];
"#;
    assert_eq!(compile_and_run(source), "4");
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map").count(), 1, "{main}");
}

/// A capturing closure keeps the map because the capture is observable state.
#[test]
fn test_map_read_capturing_callback_keeps_generic_map() {
    let source = r#"<?php
$factor = 3;
$values = [1, 2];
$mapped = array_map(fn($v) => $v * $factor, $values);
echo $mapped[1];
"#;
    assert_eq!(compile_and_run(source), "6");
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map").count(), 1, "{main}");
}

/// A division callback may throw, so it keeps the map.
#[test]
fn test_map_read_division_callback_keeps_generic_map() {
    let source = r#"<?php
$values = [2, 4];
$mapped = array_map(fn($v) => intdiv($v, 2), $values);
echo $mapped[1];
"#;
    assert_eq!(compile_and_run(source), "2");
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map").count(), 1, "{main}");
}

/// A read inside a later block keeps the map because the temporary escapes the straight line.
#[test]
fn test_map_read_after_block_keeps_generic_map() {
    let source = r#"<?php
$values = [1, 2];
$mapped = array_map(fn($v) => $v * 2, $values);
if ($argc > 0) { echo $mapped[1]; }
"#;
    assert_eq!(compile_and_run(source), "4");
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map").count(), 1, "{main}");
}
