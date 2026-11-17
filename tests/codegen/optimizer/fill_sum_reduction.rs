//! Purpose:
//! End-to-end coverage for the closed-form modular fill+sum and arithmetic-series reductions.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - The matched six-statement sequence disappears from the EIR and prints the exact
//!   mathematical sum of `i % D` for `i` in `0..N`.
//! - An observed array, a variable divisor, a non-zero seed, and a zero-trip loop keep the
//!   original loops and PHP's results.

use crate::support::*;

/// Emits the optimized EIR text for one fixture.
fn optimized_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_fill_sum_reduction_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write fill sum reduction fixture");
    let output = elephc_cli_command(&dir)
        .arg("--emit-ir")
        .arg(&path)
        .output()
        .expect("run elephc --emit-ir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(dir);
    String::from_utf8(output.stdout).expect("EIR is UTF-8")
}

/// The benchmark-shaped fill+sum collapses to one constant.
#[test]
fn test_fill_sum_reduction_benchmark_shape() {
    let source = r#"<?php
$data = [];
$i = 0;
while ($i < 50000) {
    $data[] = $i % 97;
    $i += 1;
}
$sum = 0;
$i = 0;
while ($i < count($data)) {
    $sum += $data[$i];
    $i += 1;
}
echo $sum, "\n";
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2398830\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
    let ir = optimized_ir(source);
    assert!(!ir.contains("array_push"), "{ir}");
    assert!(!ir.contains("while.cond"), "{ir}");
}

/// A zero-trip fill and a unit divisor both sum to zero.
#[test]
fn test_fill_sum_reduction_edge_bounds() {
    let zero_trip = r#"<?php
$data = [];
$i = 0;
while ($i < 0) {
    $data[] = $i % 3;
    $i += 1;
}
$sum = 0;
$i = 0;
while ($i < count($data)) {
    $sum += $data[$i];
    $i += 1;
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(zero_trip), "0\n");
    let unit_divisor = r#"<?php
$data = [];
$i = 0;
while ($i < 17) {
    $data[] = $i % 1;
    $i += 1;
}
$sum = 0;
$i = 0;
while ($i < count($data)) {
    $sum += $data[$i];
    $i += 1;
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(unit_divisor), "0\n");
}

/// An array observed after the sum keeps both loops.
#[test]
fn test_fill_sum_reduction_keeps_observed_array() {
    let source = r#"<?php
$data = [];
$i = 0;
while ($i < 20) {
    $data[] = $i % 7;
    $i += 1;
}
$sum = 0;
$i = 0;
while ($i < count($data)) {
    $sum += $data[$i];
    $i += 1;
}
echo $sum, ":", count($data), "\n";
"#;
    assert_eq!(compile_and_run(source), "57:20\n");
    let ir = optimized_ir(source);
    assert!(ir.contains("array_push"), "{ir}");
}

/// A runtime divisor and a non-zero seed keep the original loops.
#[test]
fn test_fill_sum_reduction_keeps_runtime_shape() {
    let divisor = r#"<?php
$d = $argc + 96;
$data = [];
$i = 0;
while ($i < 20) {
    $data[] = $i % $d;
    $i += 1;
}
$sum = 0;
$i = 0;
while ($i < count($data)) {
    $sum += $data[$i];
    $i += 1;
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(divisor), "190\n");
    let seed = r#"<?php
$data = [];
$i = 0;
while ($i < 17) {
    $data[] = $i % 5;
    $i += 1;
}
$sum = 5;
$i = 0;
while ($i < count($data)) {
    $sum += $data[$i];
    $i += 1;
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(seed), "36\n");
}

/// A `$sum += $i` arithmetic progression collapses and leaves the counter at its final value.
#[test]
fn test_arithmetic_sum_reduction_collapses() {
    let source = r#"<?php
$sum = 0;
$i = 1;
while ($i <= 200000) {
    $sum += $i;
    $i += 1;
}
echo $sum, ":", $i, "\n";
"#;
    assert_eq!(compile_and_run(source), "20000100000:200001\n");
    let ir = optimized_ir(source);
    assert!(!ir.contains("while.cond"), "{ir}");
}

/// A single-iteration series and a strict bound both keep PHP's final counter and sum.
#[test]
fn test_arithmetic_sum_reduction_edges() {
    let single = r#"<?php
$sum = 0;
$i = 5;
while ($i <= 5) {
    $sum += $i;
    $i += 1;
}
echo $sum, ":", $i, "\n";
"#;
    assert_eq!(compile_and_run(single), "5:6\n");
    let strict = r#"<?php
$sum = 0;
$i = 0;
while ($i < 5) {
    $sum += $i;
    $i += 1;
}
echo $sum, ":", $i, "\n";
"#;
    assert_eq!(compile_and_run(strict), "10:5\n");
}

/// A non-zero seed, a runtime bound, or an overflowing total keeps the loop.
#[test]
fn test_arithmetic_sum_reduction_keeps_unproven_shapes() {
    let seeded = r#"<?php
$sum = 1;
$i = 1;
while ($i <= 10) {
    $sum += $i;
    $i += 1;
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(seeded), "56\n");
    let runtime = r#"<?php
$n = $argc + 9;
$sum = 0;
$i = 1;
while ($i <= $n) {
    $sum += $i;
    $i += 1;
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(runtime), "55\n");
    let overflowing = r#"<?php
$sum = 0;
$i = 1;
while ($i <= PHP_INT_MAX) {
    $sum += $i;
    $i += 1;
}
echo $sum, "\n";
"#;
    let ir = optimized_ir(overflowing);
    assert!(ir.contains("while.cond"), "{ir}");
}

/// A shared accumulator/counter controls the loop test and keeps the original loops.
#[test]
fn test_fill_sum_reduction_keeps_shared_counter() {
    let source = r#"<?php
$data = [];
$i = 0;
while ($i < 2) {
    $data[] = $i % 2;
    $i += 1;
}
$s = 0;
$s = 0;
while ($s < count($data)) {
    $s += $data[$s];
    $s += 1;
}
echo $s, "\n";
"#;
    assert_eq!(compile_and_run(source), "3\n");
    let ir = optimized_ir(source);
    assert!(ir.contains("while.cond"), "{ir}");
}

/// `get_defined_vars()` observes the filled array and keeps both loops.
#[test]
fn test_fill_sum_reduction_keeps_defined_vars_read() {
    let source = r#"<?php
$data = [];
$i = 0;
while ($i < 2) {
    $data[] = $i % 2;
    $i += 1;
}
$sum = 0;
$i = 0;
while ($i < count($data)) {
    $sum += $data[$i];
    $i += 1;
}
$vars = get_defined_vars();
echo $sum, ":", implode(",", $vars['data']), "\n";
"#;
    assert_eq!(compile_and_run(source), "1:0,1\n");
}
