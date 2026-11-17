//! Purpose:
//! Regression coverage for lowering `count(array_map(...))` to the source length.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Only a capture-free, untyped arithmetic closure over a packed `array<int>` qualifies.
//! - Capturing and otherwise effectful callbacks keep the materialized map and its calls.

use crate::support::*;

/// Emits the optimized `main` function body for one fixture.
fn main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_map_count_fusion_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write map-count fixture");
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

/// Add, subtract, and multiply closures all observe only the source length.
#[test]
fn test_count_map_pure_closure_uses_source_length() {
    let source = r#"<?php
$a = range(1, 10);
echo count(array_map(fn($x) => $x + 2, $a)), '|';
echo count(array_map(fn($x) => $x - 2, $a)), '|';
echo count(array_map(fn($x) => $x * 3, $a));
"#;
    assert_eq!(compile_and_run(source), "10|10|10");
    let main = main_ir(source);
    assert_array_len(&main, 3);
    assert!(!main.contains("runtime.array_map"), "{main}");
}

/// A capturing callback still runs once per element even when only its count is read.
#[test]
fn test_count_map_capturing_callback_keeps_generic_path() {
    let source = r#"<?php
$calls = 0;
$mapped = array_map(function ($x) use (&$calls) { $calls++; return $x + 1; }, [1, 2, 3]);
echo count($mapped), ':', $calls;
"#;
    assert_eq!(compile_and_run(source), "3:3");
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map").count(), 1, "{main}");
}

/// A division callback may throw and must keep the generic map.
#[test]
fn test_count_map_division_callback_keeps_generic_path() {
    let source = r#"<?php
$a = range(1, 4);
echo count(array_map(fn($x) => intdiv($x, 2), $a));
"#;
    assert_eq!(compile_and_run(source), "4");
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map").count(), 1, "{main}");
}

/// Asserts that the fused count uses the source length rather than a materialized map.
fn assert_array_len(main: &str, expected: usize) {
    assert_eq!(main.matches("array_len").count(), expected, "{main}");
}
