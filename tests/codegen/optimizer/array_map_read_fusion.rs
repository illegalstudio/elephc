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

/// A direct `array_map(...)[$key]` read rewrites to a guarded source read without materializing
/// the temporary, for both literal and simple variable indices.
#[test]
fn test_direct_map_read_literal_and_variable_index_fuse() {
    let source = r#"<?php
$values = range(1, 10);
echo array_map(fn($v) => $v * 3, $values)[2], '|';
echo array_map(fn($v) => ($v + 1) * 2, $values)[9], '|';
$index = 4;
echo array_map(fn($v) => $v - 1, $values)[$index], '|';
echo implode(',', [array_map(fn($v) => $v + 2, $values)[0], array_map(fn($v) => $v + 2, $values)[1]]);
"#;
    assert_eq!(compile_and_run(source), "9|22|4|3,4");
    let main = main_ir(source);
    assert!(!main.contains("runtime.array_map"), "{main}");
    assert_eq!(main.matches("array_isset").count(), 5, "{main}");
}

/// A missing direct-read key keeps PHP's warning and null result without invoking the callback.
#[test]
fn test_direct_map_read_missing_key_warns_and_returns_null() {
    let out = compile_and_run_capture(
        r#"<?php
$values = range(1, 3);
var_dump(array_map(fn($v) => $v * 2, $values)[5]);
"#,
    );
    assert_eq!(out.stdout, "NULL\n");
    assert!(out.stderr.contains("Undefined array key 5"), "{}", out.stderr);
}

/// A source write before the direct read is observed because the fusion evaluates the source at
/// the original expression position.
#[test]
fn test_direct_map_read_observes_earlier_source_write() {
    let out = compile_and_run(
        r#"<?php
$values = [1, 2];
$values[0] = 9;
echo array_map(fn($v) => $v * 2, $values)[0];
"#,
    );
    assert_eq!(out, "18");
}

/// Typed and effectful direct callbacks keep the generic map because an overflow must throw.
#[test]
fn test_direct_map_read_typed_and_division_callbacks_keep_generic_map() {
    let source = r#"<?php
$values = range(1, 8);
echo array_map(fn($v): int => $v * 2, $values)[1], '|';
echo array_map(fn($v) => intdiv($v, 2), $values)[1];
"#;
    assert_eq!(compile_and_run(source), "4|1");
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map").count(), 2, "{main}");
}

/// A reference alias created before the map keeps the generic map: PHP snapshots the source at
/// the assignment, so a later write through the alias must not change the read value.
#[test]
fn test_map_read_alias_before_assignment_keeps_generic_map() {
    let source = r#"<?php
$values = [1, 2];
$alias = &$values;
$mapped = array_map(fn($v) => $v * 2, $values);
$alias[0] = 99;
echo $mapped[0];
"#;
    assert_eq!(compile_and_run(source), "2");
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map").count(), 1, "{main}");
}

/// A value-`foreach` over a pure `array_map()` iterates the source with a generated value local
/// and assigns the substituted callback expression to the original loop variable, preserving
/// keys without materializing the mapped array.
#[test]
fn test_foreach_map_pure_closure_fuses() {
    let source = r#"<?php
$values = range(1, 5);
$sum = 0;
foreach (array_map(fn($v) => $v * 2, $values) as $x) {
    $sum += $x;
}
echo $sum, "|";
$keys = [];
$more = [7, 8];
foreach (array_map(fn($v) => $v + 10, $more) as $k => $x) {
    $keys[] = $k . ":" . $x;
}
echo implode(",", $keys);
"#;
    assert_eq!(compile_and_run(source), "30|0:17,1:18");
    let main = main_ir(source);
    assert!(!main.contains("runtime.array_map"), "{main}");
}

/// The original loop variable keeps its final mapped value and an exhausted empty source leaves
/// it unchanged, exactly like PHP's `foreach`.
#[test]
fn test_foreach_map_preserves_final_value_and_empty_source() {
    let source = r#"<?php
$values = [1, 2, 3];
$x = "before";
foreach (array_map(fn($v) => $v * 2, $values) as $x) {}
echo $x, "|";
$empty = [];
$x = "kept";
foreach (array_map(fn($v) => $v * 2, $empty) as $x) {}
echo $x;
"#;
    assert_eq!(compile_and_run(source), "6|kept");
}

/// A body that writes the loop variable or passes it by reference keeps the exact per-iteration
/// assignment semantics because the loop variable remains a real Mixed local.
#[test]
fn test_foreach_map_body_value_writes_and_by_reference_use() {
    let source = r#"<?php
function bump(&$n): void { $n += 10; }
$values = [1, 2, 3];
$sum = 0;
foreach (array_map(fn($v) => $v + 1, $values) as $x) {
    $x = $x + 100;
    $sum += $x;
}
echo $sum, "|", $x, "|";
$sum = 0;
foreach (array_map(fn($v) => $v * 2, $values) as $x) {
    bump($x);
    $sum += $x;
}
echo $sum, "|", $x;
"#;
    assert_eq!(compile_and_run(source), "309|104|42|16");
    let main = main_ir(source);
    assert!(!main.contains("runtime.array_map"), "{main}");
}

/// An overflowing mapped value is still promoted to float before the loop body observes it.
#[test]
fn test_foreach_map_overflow_promotes_to_float() {
    let source = r#"<?php
$big = [PHP_INT_MAX];
foreach (array_map(fn($v) => $v + 1, $big) as $x) {
    echo gettype($x), "|", $x;
}
"#;
    assert_eq!(
        compile_and_run(source),
        "double|9.2233720368548E+18"
    );
    let main = main_ir(source);
    assert!(!main.contains("runtime.array_map"), "{main}");
}

/// An array push into a local initialized before the loop preserves the accumulated elements and
/// still fuses the map; the destination remains a real array across iterations.
#[test]
fn test_foreach_map_initialized_array_push_fuses() {
    let source = r#"<?php
$values = [1, 2, 3];
$seen = [];
foreach (array_map(fn($v) => $v * 2, $values) as $x) {
    $seen[] = $x;
}
echo implode(",", $seen);
"#;
    assert_eq!(compile_and_run(source), "2,4,6");
    let main = main_ir(source);
    assert!(!main.contains("runtime.array_map"), "{main}");
}

/// A loop body that mutates the map source keeps the generic map because PHP iterates the
/// detached mapped snapshot.
#[test]
fn test_foreach_map_body_source_write_keeps_generic_map() {
    let source = r#"<?php
$values = [1, 2, 3];
$sum = 0;
foreach (array_map(fn($v) => $v * 2, $values) as $x) {
    $values[0] = 99;
    $sum += $x;
}
echo $sum;
"#;
    assert_eq!(compile_and_run(source), "12");
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map").count(), 1, "{main}");
}

/// A reference alias to the source keeps the generic map: a write through the alias during the
/// body must not change later mapped values.
#[test]
fn test_foreach_map_aliased_source_keeps_generic_map() {
    let source = r#"<?php
$values = [1, 2, 3];
$alias = &$values;
$sum = 0;
foreach (array_map(fn($v) => $v * 2, $values) as $x) {
    $alias[0] = 99;
    $sum += $x;
}
echo $sum;
"#;
    assert_eq!(compile_and_run(source), "12");
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map").count(), 1, "{main}");
}

/// Iterating the mapped temporary by reference and a typed callback return both keep the
/// generic map.
#[test]
fn test_foreach_map_by_reference_and_typed_return_keep_generic_map() {
    let source = r#"<?php
$values = [1, 2];
$out = [];
foreach (array_map(fn($v) => $v * 2, $values) as &$x) {
    $out[] = $x;
}
unset($x);
echo implode(",", $out), "|";
foreach (array_map(fn(int $v): int => $v * 2, $values) as $x) {
    echo $x;
}
"#;
    assert_eq!(compile_and_run(source), "2,4|24");
    let main = main_ir(source);
    assert_eq!(main.matches("runtime.array_map").count(), 2, "{main}");
}

/// The generated iteration local never leaks into `get_defined_vars()`.
#[test]
fn test_foreach_map_generated_iteration_local_is_hidden() {
    let source = r#"<?php
$values = [1, 2, 3];
$sum = 0;
foreach (array_map(fn($v) => $v * 2, $values) as $x) {
    $sum += $x;
}
$keys = implode(",", array_keys(get_defined_vars()));
echo $keys;
"#;
    let out = compile_and_run(source);
    assert!(out.contains("values"), "{out}");
    assert!(out.contains("sum"), "{out}");
    assert!(out.contains("x"), "{out}");
    assert!(!out.contains("foreach_map_value"), "{out}");
}

/// Compiles and runs one fixture through the CLI with an explicit EIR optimizer mode.
fn run_with_ir_opt(source: &str, mode: &str) -> String {
    let dir = make_cli_test_dir("elephc_map_foreach_ir_opt");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write map foreach IR-opt fixture");
    let compiled = elephc_cli_command(&dir)
        .arg(mode)
        .arg(&php_path)
        .output()
        .expect("compile map foreach IR-opt fixture");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = std::process::Command::new(dir.join("main"))
        .output()
        .expect("run map foreach IR-opt fixture");
    let _ = fs::remove_dir_all(dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("map foreach output is UTF-8")
}

/// The fused loop produces the same output with the EIR optimizer on and off.
#[test]
fn test_foreach_map_ir_opt_parity() {
    let source = r#"<?php
$values = range(1, 100);
$sum = 0;
for ($i = 0; $i < 2; $i++) {
    foreach (array_map(fn($v) => $v * 3, $values) as $x) {
        $sum += $x;
    }
}
echo $sum;
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "30300");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "30300");
}

/// The fused loop retires its generated iteration local and every mapped value without leaks.
#[test]
fn test_foreach_map_heap_debug_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = range(1, 10);
$sum = 0;
foreach (array_map(fn($v) => $v * 3, $values) as $x) {
    $sum += $x;
}
echo $sum;
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "165");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}
