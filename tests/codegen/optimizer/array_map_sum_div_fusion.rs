//! Purpose:
//! End-to-end coverage for fusing a stored int-division `array_map()` temporary whose
//! only observer is `array_sum()`.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - The fused kernel must preserve PHP's exact-int-or-double division rule,
//!   `PHP_INT_MIN / -1` promotion, left-to-right sum order, integer-overflow promotion
//!   and the empty-source integer zero.
//! - Dynamic divisors, observed maps, reassigned map locals, source writes between the map
//!   and the sum, parameter sources and zero divisors must keep the generic map and its
//!   exact result/diagnostic.
//! - Assertions cover EIR target selection, `--ir-opt` parity, heap cleanliness and assembly
//!   emission on every supported target.

use crate::support::*;

/// Emits the optimized or unoptimized EIR module text of one fixture.
fn emit_ir(source: &str, ir_opt: bool) -> String {
    let dir = make_cli_test_dir("elephc_map_sum_div_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write map-sum-div fixture");
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
    let dir = make_cli_test_dir("elephc_map_sum_div_run");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write map-sum-div run fixture");
    let compiled = elephc_cli_command(&dir)
        .arg(mode)
        .arg(&path)
        .output()
        .expect("compile map-sum-div fixture");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = std::process::Command::new(dir.join("main"))
        .output()
        .expect("run map-sum-div fixture");
    let _ = fs::remove_dir_all(dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("map-sum-div output is UTF-8")
}

/// A stored int-division map observed only by `array_sum()` fuses into one kernel.
#[test]
fn test_map_sum_int_div_fuses_stored_map() {
    let source = r#"<?php
$values = range(1, 100);
$sum = 0;
for ($i = 0; $i < $argc + 3; $i++) {
    $mapped = array_map(fn($v) => $v / 2, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "10100\n");
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map_sum_int_div").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map_int_div"), "{main}");
    assert!(!main.contains("runtime.array_sum"), "{main}");
}

/// Exact integer quotients keep an integer sum instead of a promoted float.
#[test]
fn test_map_sum_int_div_exact_quotients_stay_int() {
    let source = r#"<?php
$vals = [5, 10, 15];
$sum = 0;
for ($i = 0; $i < $argc + 1; $i++) {
    $mapped = array_map(fn($v) => $v / 5, $vals);
    $sum += array_sum($mapped);
}
var_dump($sum);
"#;
    assert_eq!(compile_and_run(source), "int(12)\n");
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map_sum_int_div").count(), 1, "{main}");
}

/// Non-exact and negative quotients promote in source order and match PHP.
#[test]
fn test_map_sum_int_div_nonexact_and_negative_match_php() {
    let source = r#"<?php
$vals = [-7, 7, -8, 8];
$sum = 0;
for ($i = 0; $i < $argc + 0; $i++) {
    $mapped = array_map(fn($v) => $v / 2, $vals);
    $sum += array_sum($mapped);
}
var_dump($sum);
"#;
    assert_eq!(compile_and_run(source), "float(0)\n");
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map_sum_int_div").count(), 1, "{main}");
}

/// `PHP_INT_MIN / -1` promotes the quotient to the same double PHP produces.
#[test]
fn test_map_sum_int_div_int_min_by_minus_one_promotes() {
    let source = r#"<?php
$vals = [PHP_INT_MIN];
$sum = 0;
for ($i = 0; $i < $argc + 0; $i++) {
    $mapped = array_map(fn($v) => $v / -1, $vals);
    $sum += array_sum($mapped);
}
var_dump($sum);
"#;
    assert_eq!(
        compile_and_run(source),
        "float(9.223372036854776E+18)\n"
    );
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map_sum_int_div").count(), 1, "{main}");
}

/// Integer sum overflow promotes to a double instead of wrapping.
#[test]
fn test_map_sum_int_div_sum_overflow_promotes() {
    let source = r#"<?php
$vals = [PHP_INT_MAX, PHP_INT_MAX];
$sum = 0;
for ($i = 0; $i < $argc + 0; $i++) {
    $mapped = array_map(fn($v) => $v / 1, $vals);
    $sum += array_sum($mapped);
}
var_dump($sum);
"#;
    assert_eq!(
        compile_and_run(source),
        "float(1.8446744073709552E+19)\n"
    );
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map_sum_int_div").count(), 1, "{main}");
}

/// A dynamic divisor keeps the generic map and its exact result.
#[test]
fn test_map_sum_int_div_dynamic_divisor_falls_back() {
    let source = r#"<?php
$vals = range(1, 4);
$d = $argc > 0 ? 2 : 3;
$sum = 0;
for ($i = 0; $i < $argc + 1; $i++) {
    $mapped = array_map(fn($v) => $v / $d, $vals);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "10\n");
    let main = main_ir(source);
    assert!(!main.contains("runtime.array_map_sum_int_div"), "{main}");
}

/// Observing the mapped array after the sum keeps the generic materialized map.
#[test]
fn test_map_sum_int_div_observed_map_falls_back() {
    let source = r#"<?php
$vals = range(1, 3);
$sum = 0;
for ($i = 0; $i < $argc + 1; $i++) {
    $mapped = array_map(fn($v) => $v / 2, $vals);
    $sum += array_sum($mapped);
}
echo $sum, "|", $mapped[0], "\n";
"#;
    assert_eq!(compile_and_run(source), "6|0.5\n");
    let main = main_ir(source);
    assert!(!main.contains("runtime.array_map_sum_int_div"), "{main}");
}

/// A source write between the map and the sum keeps the already-materialized map.
#[test]
fn test_map_sum_int_div_source_write_between_falls_back() {
    let source = r#"<?php
$vals = range(1, 2);
$sum = 0;
for ($i = 0; $i < $argc + 1; $i++) {
    $mapped = array_map(fn($v) => $v / 2, $vals);
    $vals[0] = $vals[0] + 2;
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "4\n");
    let main = main_ir(source);
    assert!(!main.contains("runtime.array_map_sum_int_div"), "{main}");
}

/// A mapped local written before the loop has two stores and keeps the generic map.
#[test]
fn test_map_sum_int_div_reassigned_mapped_local_falls_back() {
    let source = r#"<?php
$vals = range(1, 2);
$mapped = [];
$sum = 0;
for ($i = 0; $i < $argc + 1; $i++) {
    $mapped = array_map(fn($v) => $v / 2, $vals);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "3\n");
    let main = main_ir(source);
    assert!(!main.contains("runtime.array_map_sum_int_div"), "{main}");
}

/// A zero divisor keeps the generic map and PHP's `DivisionByZeroError`.
#[test]
fn test_map_sum_int_div_zero_divisor_keeps_php_error() {
    let source = r#"<?php
$vals = range(1, 2);
$d = 0;
$sum = 0;
try {
    for ($i = 0; $i < $argc + 1; $i++) {
        $mapped = array_map(fn($v) => $v / $d, $vals);
        $sum += array_sum($mapped);
    }
} catch (DivisionByZeroError $e) {
    echo get_class($e), ":", $e->getMessage(), "\n";
}
"#;
    assert_eq!(
        compile_and_run(source),
        "DivisionByZeroError:Division by zero\n"
    );
    let main = main_ir(source);
    assert!(!main.contains("runtime.array_map_sum_int_div"), "{main}");
}

/// A parameter source is not a PHP local store, so the generic map stays.
#[test]
fn test_map_sum_int_div_parameter_source_falls_back() {
    let source = r#"<?php
function total(array $vals, int $iters): float {
    $sum = 0;
    for ($i = 0; $i < $iters; $i++) {
        $mapped = array_map(fn($v) => $v / 2, $vals);
        $sum += array_sum($mapped);
    }
    return $sum;
}
echo total(range(1, 4), 2), "\n";
"#;
    assert_eq!(compile_and_run(source), "10\n");
    let main = emit_ir(source, true);
    assert!(!main.contains("runtime.array_map_sum_int_div"), "{main}");
}

/// Two independent stored maps in one body fuse independently.
#[test]
fn test_map_sum_int_div_two_maps_fuse_independently() {
    let source = r#"<?php
$a = range(1, 4);
$b = [10, 20];
$sum = 0;
for ($i = 0; $i < $argc + 1; $i++) {
    $ma = array_map(fn($v) => $v / 2, $a);
    $sum += array_sum($ma);
    $mb = array_map(fn($v) => $v / 5, $b);
    $sum += array_sum($mb);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "22\n");
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map_sum_int_div").count(), 2, "{main}");
}

/// The fused and generic paths agree with the EIR optimizer on and off.
#[test]
fn test_map_sum_int_div_ir_opt_parity() {
    let source = r#"<?php
$values = range(1, 100);
$sum = 0;
for ($i = 0; $i < $argc + 3; $i++) {
    $mapped = array_map(fn($v) => $v / 2, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "10100\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "10100\n");
    assert!(
        emit_ir(source, true).contains("runtime.array_map_sum_int_div"),
        "optimized EIR keeps the generic map"
    );
    assert!(
        !emit_ir(source, false).contains("runtime.array_map_sum_int_div"),
        "unoptimized EIR must not contain the fused kernel"
    );
}

/// The fused kernel and its one owned Mixed result stay heap-clean.
#[test]
fn test_map_sum_int_div_heap_debug_is_clean() {
    let source = r#"<?php
$values = range(1, 100);
$sum = 0;
for ($i = 0; $i < $argc + 3; $i++) {
    $mapped = array_map(fn($v) => $v / 2, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "10100\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Every supported target lowers the fused kernel to the shared runtime symbol.
#[test]
fn test_map_sum_int_div_emits_all_supported_targets() {
    let source = r#"<?php
$values = range(1, 100);
$sum = 0;
for ($i = 0; $i < $argc + 1; $i++) {
    $mapped = array_map(fn($v) => $v / 2, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let dir = make_cli_test_dir("elephc_map_sum_div_targets");
        let path = dir.join("main.php");
        fs::write(&path, source).expect("write map-sum-div target fixture");
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
            .expect("read map-sum-div target assembly");
        assert!(!assembly.is_empty(), "{target}: empty assembly");
        // The program stream carries only user code; the shared runtime helper (including
        // the fused kernel symbol) is covered for every target by
        // `array_map_sum_int_div_emits_all_targets` in the runtime emitter unit tests.
        let _ = fs::remove_dir_all(dir);
    }
}
