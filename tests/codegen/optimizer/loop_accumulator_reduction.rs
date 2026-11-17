//! Purpose:
//! End-to-end coverage for reducing a counted accumulator loop to its exact integer product.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Integer addends take the guarded fast path; float addends, non-zero accumulator seeds,
//!   overflow, and zero-trip bounds keep the original loop and its PHP promotion order.
//! - The loop counter must land on the same final value as the original `for`.

use crate::support::*;

/// Emits the optimized EIR text for one fixture.
fn optimized_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_loop_accumulator_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write loop accumulator fixture");
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

/// Compiles and runs one fixture with an explicit EIR optimizer mode.
fn run_with_ir_opt(source: &str, mode: &str) -> String {
    let dir = make_cli_test_dir("elephc_loop_accumulator_run");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write loop accumulator fixture");
    let compiled = elephc_cli_command(&dir)
        .arg(mode)
        .arg(&path)
        .output()
        .expect("compile loop accumulator fixture");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let run = run_binary(&dir.join("main"), &dir);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8(run.stdout).expect("stdout is UTF-8");
    let _ = fs::remove_dir_all(dir);
    stdout
}

/// An invariant quotient accumulates to its exact integer product.
#[test]
fn test_loop_accumulator_reduction_invariant_division() {
    let source = r#"<?php
$sum = 0;
$a = $argc + 5;
$b = $argc + 2;
for ($i = 0; $i < 4999999 + $argc; $i++) {
    $sum += $a / $b;
}
echo $sum, "|", $i, "\n";
"#;
    let expected = "10000000|5000000\n";
    assert_eq!(compile_and_run(source), expected);
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), expected);
    let ir = optimized_ir(source);
    // The guarded exact-product path distinguishes integer and float quotients without an
    // `intdiv()` round-trip; `is_finite()` proves the floating-point branch was emitted.
    assert!(ir.contains("runtime.is_finite"), "missing guarded exact product: {ir}");
}

/// A constant addend reduces even when the accumulator is initialized several statements earlier.
#[test]
fn test_loop_accumulator_reduction_constant_addend() {
    let source = r#"<?php
$n = 0;
$noise = 7;
for ($i = 0; $i < 4999999 + $argc; $i++) {
    $n += intval("42");
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "210000000\n");
    let ir = optimized_ir(source);
    assert!(ir.contains("runtime.is_finite"), "missing guarded exact product: {ir}");
}

/// A pure mbstring producer is evaluated once and the invariant observation reduces.
#[test]
fn test_loop_accumulator_reduction_mbstring_producer() {
    let source = r#"<?php
$input = "ＡＢＣ１２３ａｂｃ";
$limit = 32;
$n = 0;
for ($i = 0; $i < $limit; $i++) {
    $converted = mb_convert_kana($input, "a", "UTF-8");
    $n += strlen($converted) + ord($converted[0]);
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "2368\n");
    let ir = optimized_ir(source);
    assert!(ir.contains("runtime.is_finite"), "missing guarded exact product: {ir}");
}

/// A float addend keeps the per-iteration loop and PHP's repeated-addition rounding.
#[test]
fn test_loop_accumulator_reduction_keeps_float_addend() {
    let source = r#"<?php
$sum = 0.0;
for ($i = 0; $i < 16 + $argc; $i++) {
    $sum += 0.1;
}
echo $sum, "\n";
"#;
    let expected = run_with_ir_opt(source, "--ir-opt=off");
    assert_eq!(compile_and_run(source), expected);
}

/// A non-zero accumulator seed keeps the loop so intermediate overflow order is preserved.
#[test]
fn test_loop_accumulator_reduction_keeps_nonzero_seed() {
    let source = r#"<?php
$sum = PHP_INT_MAX - 2;
for ($i = 0; $i < 4 + $argc; $i++) {
    $sum += 1;
}
echo $sum, "\n";
"#;
    let expected = run_with_ir_opt(source, "--ir-opt=off");
    assert_eq!(compile_and_run(source), expected);
}

/// A zero-trip loop leaves the counter at its seed and never evaluates the producer.
#[test]
fn test_loop_accumulator_reduction_zero_trip_keeps_counter() {
    let source = r#"<?php
$n = 0;
for ($i = 0; $i < $argc - 10; $i++) {
    $n += 42;
}
echo $n, "|", $i, "\n";
"#;
    assert_eq!(compile_and_run(source), "0|0\n");
}

/// A period-4 `mb_chr()` length table reduces to a closed-form sum.
#[test]
fn test_loop_accumulator_reduction_periodic_mb_chr_len() {
    let source = r#"<?php
$codepoints = [0x41, 0xA9, 0x20AC, 0x1F600];
$n = 0;
for ($i = 0; $i < 32 + $argc; $i++) {
    $encoded = mb_chr($codepoints[$i & 3], "UTF-8");
    $n += strlen($encoded);
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "81\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "81\n");
    let ir = optimized_ir(source);
    assert!(ir.contains("intdiv"), "{ir}");
}

/// The length plus first-byte observation also reduces over the same table.
#[test]
fn test_loop_accumulator_reduction_periodic_mb_chr_first_byte() {
    let source = r#"<?php
$codepoints = [0x41, 0xA9, 0x20AC, 0x1F600];
$n = 0;
for ($i = 0; $i < 32 + $argc; $i++) {
    $encoded = mb_chr($codepoints[$i & 3], "UTF-8");
    $n += strlen($encoded) + ord($encoded[0]);
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "5946\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "5946\n");
}

/// An invalid codepoint in the literal table keeps the original loop and PHP's false result.
#[test]
fn test_loop_accumulator_reduction_periodic_invalid_codepoint_falls_back() {
    let source = r#"<?php
$codepoints = [0xD800, 0x41];
$n = 0;
for ($i = 0; $i < 16 + $argc; $i++) {
    $encoded = mb_chr($codepoints[$i & 1], "UTF-8");
    $n += strlen($encoded);
}
echo $n, "\n";
"#;
    let expected = run_with_ir_opt(source, "--ir-opt=off");
    assert_eq!(compile_and_run(source), expected);
}

/// An integer-valued float addend reduces to one product inside the exact-double bound.
#[test]
fn test_loop_accumulator_reduction_integer_valued_float_addend() {
    let source = r#"<?php
$add = 2.0 + $argc;
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $sum += $add;
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "12\n");
    let ir = optimized_ir(source);
    assert!(ir.contains("floor"), "{ir}");
}

/// The hoisted product-reduce result feeds the same exact-double loop reduction.
#[test]
fn test_loop_accumulator_reduction_integer_valued_float_reduce() {
    let source = r#"<?php
$values = range(1, 6);
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $sum += array_reduce($values, fn($c, $v) => $c * $v, 1.5);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "4320\n");
    let ir = optimized_ir(source);
    assert!(ir.contains("floor"), "{ir}");
}

/// A float product beyond 2^53 keeps the per-iteration additions.
#[test]
fn test_loop_accumulator_reduction_keeps_wide_float_product() {
    let source = r#"<?php
$add = 1.0e16;
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $sum += $add;
}
echo $sum, "\n";
"#;
    let expected = run_with_ir_opt(source, "--ir-opt=off");
    assert_eq!(compile_and_run(source), expected);
}

/// A half-integral float addend reduces inside the quarter-bit exact-double bound.
#[test]
fn test_loop_accumulator_reduction_half_integral_float_addend() {
    let source = r#"<?php
$add = 2.5 + $argc;
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $sum += $add;
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "14\n");
    let ir = optimized_ir(source);
    assert!(ir.contains("floor"), "{ir}");
}

/// A `Countable` addend calls user code per iteration and stays in the loop.
#[test]
fn test_loop_accumulator_reduction_keeps_countable_addend() {
    let source = r#"<?php
class Counter implements Countable {
    public int $value = 0;
    public function count(): int {
        return $this->value++;
    }
}
$counter = new Counter();
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $sum += count($counter);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "6\n");
    let ir = optimized_ir(source);
    assert!(ir.contains("for.body"), "{ir}");
}

/// A fractional inclusive bound runs zero times and must not evaluate the addend.
#[test]
fn test_loop_accumulator_reduction_skipped_fractional_bound() {
    let source = r#"<?php
$bound = $argc / -2;
$divisor = $argc - 1;
$sum = 0;
for ($i = 0; $i <= $bound; $i++) {
    $sum += 1 / $divisor;
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "0\n");
}

/// The periodic fast path leaves the final encoded character visible after the loop.
#[test]
fn test_loop_accumulator_reduction_periodic_keeps_final_encoded() {
    let source = r#"<?php
$codepoints = [0x41, 0xA9, 0x20AC, 0x1F600];
$encoded = mb_chr(0x41, "UTF-8");
$n = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $encoded = mb_chr($codepoints[$i & 3], "UTF-8");
    $n += strlen($encoded);
}
echo $n, ":", strlen($encoded), "\n";
"#;
    assert_eq!(compile_and_run(source), "10:4\n");
}

/// A by-reference call between the table literal and the loop blocks the literal reduction.
#[test]
fn test_loop_accumulator_reduction_keeps_table_mutated_by_call() {
    let source = r#"<?php
function widen(array &$values): int {
    $values[0] = 0x1F600;
    return 0;
}
$codepoints = [0x41, 0x42];
$changed = widen($codepoints);
$n = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $encoded = mb_chr($codepoints[$i & 1], "UTF-8");
    $n += strlen($encoded);
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "10\n");
}

/// A positive fractional bound keeps the original per-iteration loop.
#[test]
fn test_loop_accumulator_reduction_keeps_positive_fractional_bound() {
    let source = r#"<?php
$bound = $argc + 2.5;
$sum = 0;
for ($i = 0; $i <= $bound; $i++) {
    $sum += 1;
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "4\n");
}

/// A producer that reads its own destination cannot be evaluated once.
#[test]
fn test_loop_accumulator_reduction_keeps_self_reading_producer() {
    let source = r#"<?php
$text = "x";
$total = 0;
for ($i = 0; $i < 3; $i++) {
    $text = str_repeat($text, 2);
    $total += strlen($text);
}
echo $total, "\n";
"#;
    assert_eq!(compile_and_run(source), "14\n");
    let ir = optimized_ir(source);
    assert!(ir.contains("str_repeat"), "{ir}");
}

/// A warning-prone array read in a division addend is evaluated only once.
#[test]
fn test_loop_accumulator_reduction_keeps_single_warning_for_array_addend() {
    let source = r#"<?php
$values = [];
$sum = 0;
for ($i = 0; $i < $argc; $i++) {
    $sum += $values[9] / 2;
}
echo $sum, "\n";
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "0\n", "stderr: {}", out.stderr);
    assert_eq!(
        out.stderr.matches("Undefined array key 9").count(),
        1,
        "expected one warning, got: {}",
        out.stderr,
    );
}

/// An undefined division addend is read once, not once per guarded evaluation.
#[test]
fn test_loop_accumulator_reduction_keeps_single_warning_for_undefined_addend() {
    let source = r#"<?php
$sum = 0;
for ($i = 0; $i < $argc; $i++) {
    $sum += $undefined / 2;
}
echo $sum, "\n";
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "0\n", "stderr: {}", out.stderr);
    assert_eq!(
        out.stderr.matches("Undefined variable $undefined").count(),
        1,
        "expected one warning, got: {}",
        out.stderr,
    );
}
