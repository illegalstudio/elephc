//! Purpose:
//! End-to-end coverage for caching a loop-invariant `array_sum(array_map(...))` before its loop.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - The cached form executes the map and sum once, then only accumulates the hidden Mixed local;
//!   the normal local epilogue must release it (heap debug stays clean).
//! - A mapped local observed after the loop, a source written inside the loop, and a bound that
//!   may not run a single iteration must keep the per-iteration map.

use crate::support::*;

/// Emits the optimized EIR text for one fixture.
fn optimized_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_map_sum_cache_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write map sum cache fixture");
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

/// Returns the first `for` body slice of an EIR module dump.
fn first_loop_body(ir: &str) -> &str {
    ir.split("for.body")
        .nth(1)
        .expect("fixture has a for body")
        .split("for.update")
        .next()
        .expect("fixture has a for update")
}

/// A loop-invariant map+sum runs once and then only accumulates its cached Mixed value.
#[test]
fn test_map_sum_cache_hoists_invariant_pair() {
    let source = r#"<?php
$values = [1, 2, 3];
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $mapped = array_map(fn($v) => $v * 2, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "48\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    let ir = optimized_ir(source);
    let body = first_loop_body(&ir);
    assert!(!body.contains("runtime.array_map"), "{body}");
    assert!(!body.contains("runtime.array_sum"), "{body}");
}

/// A mapped local read after the loop keeps the per-iteration map.
#[test]
fn test_map_sum_cache_keeps_map_observed_after_loop() {
    let source = r#"<?php
$values = [1, 2, 3];
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $mapped = array_map(fn($v) => $v * 2, $values);
    $sum += array_sum($mapped);
}
echo $sum, "|", implode(",", $mapped), "\n";
"#;
    assert_eq!(compile_and_run(source), "48|2,4,6\n");
    let ir = optimized_ir(source);
    assert!(first_loop_body(&ir).contains("runtime.array_map"), "{ir}");
}

/// A producer argument the loop itself updates keeps the per-iteration map.
///
/// `range($n, $n)` reads the accumulator, so hoisting the source would map its first value on
/// every iteration: PHP's 81 becomes 9.
#[test]
fn test_map_sum_cache_keeps_source_argument_written_by_the_loop() {
    let source = r#"<?php
$n = 1;
for ($i = 0; $i < 3 + $argc; $i++) {
    $mapped = array_map(fn($v) => $v * 2, range($n, $n));
    $n += array_sum($mapped);
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "81\n");
    let ir = optimized_ir(source);
    assert!(first_loop_body(&ir).contains("runtime.array_map"), "{ir}");
}

/// A source written inside the loop keeps the per-iteration map.
#[test]
fn test_map_sum_cache_keeps_mutated_source() {
    let source = r#"<?php
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $values = [1, 2, 3];
    $mapped = array_map(fn($v) => $v * 2, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "48\n");
    let ir = optimized_ir(source);
    assert!(first_loop_body(&ir).contains("runtime.array_map"), "{ir}");
}

/// A bound that may run zero times keeps the map inside the loop.
#[test]
fn test_map_sum_cache_keeps_nonpositive_bound() {
    let source = r#"<?php
$values = [1, 2, 3];
$sum = 0;
for ($i = 0; $i < $argc - 10; $i++) {
    $mapped = array_map(fn($v) => $v * 2, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "0\n");
    let ir = optimized_ir(source);
    assert!(first_loop_body(&ir).contains("runtime.array_map"), "{ir}");
}

/// A constant-argument `range()` source is hoisted once with the map and sum.
#[test]
fn test_map_sum_cache_hoists_range_source() {
    let source = r#"<?php
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $mapped = array_map(fn($v) => $v * 2, range(1, 3));
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "48\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    let body = first_loop_body(&optimized_ir(source)).to_string();
    assert!(!body.contains("runtime.range"), "{body}");
    assert!(!body.contains("runtime.array_map"), "{body}");
}

/// An invariant slice source is hoisted with the map and sum.
#[test]
fn test_map_sum_cache_hoists_slice_source() {
    let source = r#"<?php
$values = [1, 2, 3, 4];
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $mapped = array_map(fn($v) => $v * 2, array_slice($values, 1, 2));
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "40\n");
    let body = first_loop_body(&optimized_ir(source)).to_string();
    assert!(!body.contains("runtime.array_slice"), "{body}");
    assert!(!body.contains("runtime.array_map"), "{body}");
}

/// A source producer that reads the loop counter keeps the per-iteration map.
#[test]
fn test_map_sum_cache_keeps_counter_dependent_source() {
    let source = r#"<?php
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $mapped = array_map(fn($v) => $v * 2, range(1, $i + 1));
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "40\n");
    let body = first_loop_body(&optimized_ir(source)).to_string();
    assert!(body.contains("runtime.range"), "{body}");
    assert!(body.contains("runtime.array_map"), "{body}");
}

/// A `foreach` over an invariant map with a single value accumulation caches the map sum.
#[test]
fn test_map_sum_cache_hoists_foreach_sum() {
    let source = r#"<?php
$values = [1, 2, 3];
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    foreach (array_map(fn($v) => $v * 2, $values) as $x) {
        $sum += $x;
    }
}
echo $sum, "\n";
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "48\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    let body = first_loop_body(&optimized_ir(source)).to_string();
    assert!(!body.contains("runtime.array_map"), "{body}");
    assert!(!body.contains("array_sum"), "{body}");
}

/// A mapped local read only through constant keys is hoisted and its loop reduced.
#[test]
fn test_map_sum_cache_hoists_constant_element_reads() {
    let source = r#"<?php
$values = [1, 2, 3];
$n = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $mapped = array_map(fn($v) => $v * 2, $values);
    $n += $mapped[0] + $mapped[2];
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "32\n");
    let body = first_loop_body(&optimized_ir(source)).to_string();
    assert!(!body.contains("runtime.array_map"), "{body}");
}

/// A function-local parameter-bounded map+sum caches once despite the checker's generic type.
#[test]
fn test_map_sum_cache_hoists_parameter_bounded_function_local() {
    let source = r#"<?php
function mapLocal(int $limit): float {
    $values = range(1, 3);
    $sum = 0;
    for ($i = 0; $i < 3 + $limit; $i++) {
        $mapped = array_map(fn($v) => $v + 1.5, $values);
        $sum += array_sum($mapped);
    }
    return $sum;
}
echo mapLocal($argc + 1), "\n";
"#;
    assert_eq!(compile_and_run(source), "52.5\n");
    let body = first_loop_body(&optimized_ir(source)).to_string();
    assert!(!body.contains("runtime.array_map"), "{body}");
}

/// A direct `array_map(...)[$k]` read is hoisted once and the loop reduces.
#[test]
fn test_map_sum_cache_hoists_direct_map_read() {
    let source = r#"<?php
$values = range(1, 100);
$n = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $n += array_map(fn($v) => $v * 3, $values)[10];
}
echo $n, "\n";
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "132\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    let body = first_loop_body(&optimized_ir(source)).to_string();
    assert!(!body.contains("runtime.array_map"), "{body}");
}

/// An invariant `array_reduce()` addend with a float seed is hoisted once.
#[test]
fn test_map_sum_cache_hoists_float_seed_reduce() {
    let source = r#"<?php
$values = range(1, 10);
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $sum += array_reduce($values, fn($c, $v) => $c + $v, 1.5);
}
echo $sum, "\n";
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "226\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    let body = first_loop_body(&optimized_ir(source)).to_string();
    assert!(!body.contains("runtime.array_reduce"), "{body}");
}

/// An invariant product reduce with a float seed keeps PHP's float rounding.
#[test]
fn test_map_sum_cache_hoists_product_reduce() {
    let source = r#"<?php
$values = range(1, 6);
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $sum += array_reduce($values, fn($c, $v) => $c * $v, 1.5);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "4320\n");
    let body = first_loop_body(&optimized_ir(source)).to_string();
    assert!(!body.contains("runtime.array_reduce"), "{body}");
}

/// A callback that captures a local keeps the per-iteration reduce.
#[test]
fn test_map_sum_cache_keeps_capturing_reduce() {
    let source = r#"<?php
$values = range(1, 10);
$bonus = 2;
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $sum += array_reduce($values, fn($c, $v) => $c + $v + $bonus, 1.5);
}
echo $sum, "\n";
"#;
    let expected = compile_and_run(source);
    let body = first_loop_body(&optimized_ir(source)).to_string();
    assert!(body.contains("runtime.array_reduce"), "{body}");
    assert!(!expected.is_empty());
}

/// A reduce callback that can divide by zero stays in the loop, even behind a positive-looking
/// bound that is zero at runtime.
#[test]
fn test_map_sum_cache_keeps_throwing_reduce_callback() {
    let source = r#"<?php
$values = [0, 1, 2];
$n = $argc - 1;
$sum = 0;
for ($i = 0; $i < $n + 0; $i++) {
    $sum += array_reduce($values, fn($c, $v) => $c / $v, 1.0);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "0\n");
    let ir = optimized_ir(source);
    let body = first_loop_body(&ir);
    assert!(body.contains("runtime.array_reduce"), "{body}");
}

/// A mutating call on the mapped array keeps the per-iteration map rebuild.
#[test]
fn test_map_sum_cache_keeps_mutated_map_assignment() {
    let source = r#"<?php
$values = [1, 2, 3];
$n = 1;
for ($i = 0; $i < 4; $i++) {
    $mapped = array_map(fn($v) => $v * 2, $values);
    $n += array_pop($mapped);
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "25\n");
    let body = first_loop_body(&optimized_ir(source)).to_string();
    assert!(body.contains("array_map"), "{body}");
}

/// The cached foreach sum keeps the final value variable.
#[test]
fn test_map_sum_cache_keeps_foreach_value_binding() {
    let source = r#"<?php
$values = [1, 2, 3];
$x = 0;
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    foreach (array_map(fn($v) => $v * 2, $values) as $x) {
        $sum += $x;
    }
}
echo $sum, ":", $x, "\n";
"#;
    assert_eq!(compile_and_run(source), "48:6\n");
}

/// The cached foreach sum keeps the final key variable.
#[test]
fn test_map_sum_cache_keeps_foreach_key_binding() {
    let source = r#"<?php
$values = [1, 2, 3];
$key = -1;
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    foreach (array_map(fn($v) => $v * 2, $values) as $key => $x) {
        $sum += $x;
    }
}
echo $sum, ":", $key, "\n";
"#;
    assert_eq!(compile_and_run(source), "48:2\n");
}

/// A float-producing callback keeps element-by-element foreach addition.
#[test]
fn test_map_sum_cache_keeps_float_foreach_sum() {
    let source = r#"<?php
$values = [1, 1];
$sum = 1.0e16;
for ($i = 0; $i < 3 + $argc; $i++) {
    foreach (array_map(fn($v) => $v * 1.0, $values) as $x) {
        $sum += $x;
    }
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "1.0E+16\n");
}

/// A float accumulator seed keeps element-by-element foreach addition.
#[test]
fn test_map_sum_cache_keeps_float_foreach_seed() {
    let source = r#"<?php
$values = [1, 1];
$sum = 1.0e16;
for ($i = 0; $i < 3 + $argc; $i++) {
    foreach (array_map(fn($v) => $v * 2, $values) as $x) {
        $sum += $x;
    }
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "1.0E+16\n");
}

/// Later statements in the outer loop body are not discarded by the foreach cache.
#[test]
fn test_map_sum_cache_keeps_later_loop_statements() {
    let source = r#"<?php
$values = [1, 2, 3];
$sum = 0;
$count = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    foreach (array_map(fn($v) => $v * 2, $values) as $x) {
        $sum += $x;
    }
    $count += 1;
}
echo $sum, ":", $count, "\n";
"#;
    assert_eq!(compile_and_run(source), "48:4\n");
}

/// A rejected candidate must not leave a rewritten sum reading an unset cache.
#[test]
fn test_map_sum_cache_rejection_keeps_original_sum() {
    let source = r#"<?php
$values = [1, 2, 3];
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $mapped = array_map(fn($v) => $v * 2, $values);
    $sum = array_sum($mapped) + count($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "15\n");
}

/// A source written by the loop update keeps the per-iteration map.
#[test]
fn test_map_sum_cache_keeps_source_written_by_update() {
    let source = r#"<?php
$values = [1];
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++, $values[] = 1) {
    $mapped = array_map(fn($v) => $v * 2, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "20\n");
}

/// A zero-trip loop does not run its map producer before the loop.
#[test]
fn test_map_sum_cache_skips_prelude_for_zero_trip_loop() {
    let source = r#"<?php
$values = [PHP_INT_MAX];
$sum = 0;
for ($i = 100; $i < 3 + $argc; $i++) {
    $mapped = array_map(fn($v): int => $v * 2, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "0\n");
}

/// A nested slice offset call keeps the source producer inside the loop.
#[test]
fn test_map_sum_cache_keeps_side_effecting_slice_offset() {
    let source = r#"<?php
function nextOffset(): int {
    static $offset = 0;
    return $offset++;
}
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $mapped = array_map(fn($v) => $v * 2, array_values(array_slice([1, 2, 3], nextOffset())));
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "28\n");
}

/// A variadic-only map callback is rejected without indexing an empty parameter list.
#[test]
fn test_map_sum_cache_keeps_variadic_only_map_callback() {
    let source = r#"<?php
$values = [1, 2, 3];
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    $mapped = array_map(fn(...$args) => 2, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "24\n");
}

/// The foreach cache also rejects variadic-only callbacks without panicking.
#[test]
fn test_map_sum_cache_keeps_variadic_only_foreach_callback() {
    let source = r#"<?php
$values = [1, 2, 3];
$sum = 0;
for ($i = 0; $i < 3 + $argc; $i++) {
    foreach (array_map(fn(...$args) => 2, $values) as $x) {
        $sum += $x;
    }
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "24\n");
}

/// A map whose source is the mapped local itself must stay inside the loop.
///
/// PHP rebuilds `$a` from its previous value on every iteration; hoisting the assignment
/// once would turn the doubling sequence into a single doubling.
#[test]
fn test_map_sum_cache_keeps_self_mapped_source() {
    let source = r#"<?php
$a = [1, 2, 3];
$acc = 0;
for ($i = 0; $i < 3; $i++) {
    $a = array_map(fn($v) => $v * 2, $a);
    $acc = $acc + $a[0];
}
echo $acc, " ", implode(",", $a), "\n";
"#;
    assert_eq!(compile_and_run(source), "14 8,16,24\n");
    let body = first_loop_body(&optimized_ir(source)).to_string();
    assert!(body.contains("runtime.array_map"), "{body}");
}

/// A prelude producer that can throw must not run when the loop is already zero-trip.
#[test]
fn test_map_sum_cache_guards_throwing_prelude_for_zero_trip_loop() {
    let source = r#"<?php
$k = -1;
$n = $argc - 1;
$sum = 0;
for ($i = 0; $i < $n + 0; $i++) {
    $mapped = array_map(fn($v) => $v * 2, array_fill(0, $k, 1));
    $sum = $sum + array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "0\n");
}

/// A seed past the bound also keeps the hoisted prelude behind the loop-entry guard.
#[test]
fn test_map_sum_cache_guards_prelude_when_seed_exceeds_bound() {
    let source = r#"<?php
$values = [1, 2, 3];
$sum = 0;
for ($i = 100; $i < 3 + $argc; $i++) {
    $mapped = array_map(fn($v) => $v * 2, $values);
    $sum = $sum + array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(compile_and_run(source), "0\n");
}
