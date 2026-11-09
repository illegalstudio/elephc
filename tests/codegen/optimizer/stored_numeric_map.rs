//! Purpose:
//! End-to-end coverage for the stored `array_map(fn($v) => $v + C | $v - C | $v * C |
//! $v * A + B | $v * $v | $v * $v + C | $v * F | $v * F + G | $v + F | $v - F |
//! $v / F, $ints)` kernels, including non-empty all-int array-literal sources, direct
//! `range()` sources, and key-renumbering `array_slice()` sources. Integer callbacks keep a
//! raw `array<int>` when no element overflows; float callbacks keep a raw `array<float>`.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Safe results are read through indexed access, `foreach`, `count`, `array_sum`, `implode`,
//!   `json_encode`, `var_dump`, `array_keys`, `array_key_exists`, and `array_is_list`.
//! - An overflowing element rebuilds the ordinary boxed Mixed array, preserving PHP's per-element
//!   int/float result.
//! - Consumers that may assume boxed Mixed slots (`array_merge`, `array_push`, user calls) keep
//!   the generic map. Any element write, direct or through an alias, also keeps the generic
//!   boxed map so COW and ownership stay on the established path.

use crate::support::*;

/// Emits the complete EIR for one fixture in the requested optimizer mode.
fn emit_ir(source: &str, ir_opt: bool) -> String {
    let dir = make_cli_test_dir("elephc_stored_numeric_map_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write stored numeric map fixture");
    let mode = if ir_opt { "--ir-opt=on" } else { "--ir-opt=off" };
    let output = elephc_cli_command(&dir)
        .arg("--emit-ir")
        .arg(mode)
        .arg(&path)
        .output()
        .expect("run elephc --emit-ir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("EIR is UTF-8");
    let _ = fs::remove_dir_all(dir);
    text
}

/// Emits the `main` function body for one fixture in the requested optimizer mode.
fn emit_main_ir(source: &str, ir_opt: bool) -> String {
    emit_ir(source, ir_opt)
        .split("function main(")
        .nth(1)
        .expect("EIR contains main")
        .split("\n  function ")
        .next()
        .expect("main body")
        .to_string()
}

/// Compiles and runs one fixture through the CLI with an explicit EIR optimizer mode.
fn run_with_ir_opt(source: &str, mode: &str) -> String {
    let dir = make_cli_test_dir("elephc_stored_numeric_map_run");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write stored numeric map run fixture");
    let compiled = elephc_cli_command(&dir)
        .arg(mode)
        .arg(&path)
        .output()
        .expect("compile stored numeric map fixture");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = std::process::Command::new(dir.join("main"))
        .output()
        .expect("run stored numeric map fixture");
    let _ = fs::remove_dir_all(dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stored numeric map output is UTF-8")
}

/// A safe packed-int map stores raw integers and feeds every storage-neutral consumer.
#[test]
fn test_stored_numeric_map_safe_result_and_consumers() {
    let source = r#"<?php
$values = range(1, 5);
$mapped = array_map(fn($v) => $v * 3, $values);
echo implode(",", $mapped), "|", count($mapped), "|", array_sum($mapped), "|";
foreach ($mapped as $v) { echo $v, ","; }
echo "|", implode(",", array_keys($mapped)), "|";
echo array_key_exists(2, $mapped) ? "yes" : "no", "|";
echo array_is_list($mapped) ? "list" : "hash", "|";
var_dump($mapped[0]);
"#;
    assert_eq!(
        compile_and_run(source),
        "3,6,9,12,15|5|45|3,6,9,12,15,|0,1,2,3,4|yes|list|int(3)\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_numeric").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// An overflowing element rebuilds the Mixed array while keeping non-overflowing elements ints.
#[test]
fn test_stored_numeric_map_overflow_preserves_php_types() {
    let source = r#"<?php
$big = [PHP_INT_MAX - 2, PHP_INT_MAX - 1, PHP_INT_MAX];
$mapped = array_map(fn($v) => $v + 1, $big);
foreach ($mapped as $v) { echo gettype($v), ":", $v, "|"; }
echo "|", array_sum($mapped), "|", implode(",", $mapped), "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "integer:9223372036854775806|integer:9223372036854775807|double:9.2233720368548E+18||2.7670116110564E+19|9223372036854775806,9223372036854775807,9.2233720368548E+18\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_numeric").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// Reading through a copy keeps the raw storage; the snapshot itself is unchanged.
#[test]
fn test_stored_numeric_map_read_only_copy_keeps_raw_storage() {
    let source = r#"<?php
$values = range(1, 5);
$mapped = array_map(fn($v) => $v * 3, $values);
$copy = $mapped;
echo $mapped[0], "|", $copy[4], "\n";
"#;
    assert_eq!(compile_and_run(source), "3|15\n");
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_numeric").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// Direct and aliased element writes keep the generic boxed map, preserving COW exactly.
#[test]
fn test_stored_numeric_map_element_writes_keep_generic_map() {
    let direct = r#"<?php
$values = range(1, 5);
$mapped = array_map(fn($v) => $v * 3, $values);
$mapped[0] = 99;
$mapped[] = 18;
echo implode(",", $mapped), "\n";
"#;
    assert_eq!(compile_and_run(direct), "99,6,9,12,15,18\n");
    let main = emit_main_ir(direct, true);
    assert_eq!(main.matches("runtime.array_map ").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map_numeric"), "{main}");

    let aliased = r#"<?php
$values = range(1, 5);
$mapped = array_map(fn($v) => $v * 3, $values);
$copy = $mapped;
$copy[0] = 99;
echo $mapped[0], "|", $copy[0], "\n";
"#;
    assert_eq!(compile_and_run(aliased), "3|99\n");
    let main = emit_main_ir(aliased, true);
    assert_eq!(main.matches("runtime.array_map ").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map_numeric"), "{main}");
}

/// `array_merge`, `array_push`, and user calls keep the generic boxed map.
#[test]
fn test_stored_numeric_map_unsafe_consumers_keep_generic_map() {
    let source = r#"<?php
function take(array $items): int { return count($items); }
$values = range(1, 5);
$left = array_map(fn($v) => $v * 3, $values);
$right = array_map(fn($v) => $v * 4, $values);
$merged = array_merge($left, $right);
$pushed_map = array_map(fn($v) => $v * 3, $values);
array_push($pushed_map, 18);
$called_map = array_map(fn($v) => $v * 3, $values);
echo implode(",", $merged), "|", implode(",", $pushed_map), "|", take($called_map), "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "3,6,9,12,15,4,8,12,16,20|3,6,9,12,15,18|5\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map ").count(), 4, "{main}");
    assert!(!main.contains("runtime.array_map_numeric"), "{main}");
}

/// Add and multiply accept either operand order; subtraction requires the parameter on the left.
#[test]
fn test_stored_numeric_map_arithmetic_orders() {
    let source = r#"<?php
$values = range(1, 3);
$a = array_map(fn($v) => $v + 10, $values);
$b = array_map(fn($v) => 10 + $v, $values);
$c = array_map(fn($v) => $v - 1, $values);
$d = array_map(fn($v) => $v * 2, $values);
$e = array_map(fn($v) => 2 * $v, $values);
echo implode(",", $a), "|", implode(",", $b), "|", implode(",", $c), "|",
    implode(",", $d), "|", implode(",", $e), "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "11,12,13|11,12,13|0,1,2|2,4,6|2,4,6\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_numeric").count(), 5, "{main}");
}

/// The fused kernel produces the same output with the EIR optimizer on and off.
#[test]
fn test_stored_numeric_map_ir_opt_parity() {
    let source = r#"<?php
$values = range(1, 100);
$sum = 0;
for ($i = 0; $i < 2; $i++) {
    $mapped = array_map(fn($v) => $v * 3, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "30300\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "30300\n");
}

/// The raw result and its Mixed overflow fallback release without leaks.
#[test]
fn test_stored_numeric_map_heap_debug_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = range(1, 5);
$mapped = array_map(fn($v) => $v * 3, $values);
$big = [PHP_INT_MAX];
$overflow = array_map(fn($v) => $v + 1, $big);
echo array_sum($mapped), "|", gettype($overflow[0]), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "45|double\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Composite affine callbacks parse in every supported operand order and evaluate exactly.
#[test]
fn test_stored_numeric_map_affine_composite_shapes() {
    let source = r#"<?php
$values = range(1, 5);
$a = array_map(fn($v) => $v * 3 + 1, $values);
$b = array_map(fn($v) => $v * 3 - 1, $values);
$c = array_map(fn($v) => 1 + $v * 3, $values);
$d = array_map(fn($v) => 10 - $v * 2, $values);
echo implode(",", $a), "|", implode(",", $b), "|", implode(",", $c), "|", implode(",", $d);
"#;
    assert_eq!(
        compile_and_run(source),
        "4,7,10,13,16|2,5,8,11,14|4,7,10,13,16|8,6,4,2,0"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_affine").count(), 4, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// Multiplication and addition promote independently, preserving PHP's per-element types.
#[test]
fn test_stored_numeric_map_affine_overflow_preserves_php_types() {
    let source = r#"<?php
$big = [PHP_INT_MAX - 1, PHP_INT_MAX, 1];
$mul = array_map(fn($v) => $v * 2 + 1, $big);
foreach ($mul as $v) { echo gettype($v), ":", $v, "|"; }
echo "\n";
$edge = [PHP_INT_MAX];
$add = array_map(fn($v) => $v * 1 + 1, $edge);
foreach ($add as $v) { echo gettype($v), ":", $v, "|"; }
echo "\n";
$min = [PHP_INT_MIN];
$negative = array_map(fn($v) => $v * 2 + 1, $min);
foreach ($negative as $v) { echo gettype($v), ":", $v, "|"; }
echo "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "double:1.844674407371E+19|double:1.844674407371E+19|integer:3|\n\
         double:9.2233720368548E+18|\n\
         double:-1.844674407371E+19|\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_affine").count(), 3, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// Safe affine results feed every storage-neutral consumer without boxing elements.
#[test]
fn test_stored_numeric_map_affine_consumers() {
    let source = r#"<?php
$values = range(1, 5);
$mapped = array_map(fn($v) => $v * 3 + 1, $values);
echo implode(",", $mapped), "|", array_sum($mapped), "|", count($mapped), "|";
foreach ($mapped as $v) { echo $v, ","; }
echo "|", implode(",", array_keys($mapped)), "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "4,7,10,13,16|50|5|4,7,10,13,16,|0,1,2,3,4\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_affine").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// Element writes keep the generic boxed map, preserving COW and ownership.
#[test]
fn test_stored_numeric_map_affine_element_writes_keep_generic_map() {
    let source = r#"<?php
$values = range(1, 3);
$mapped = array_map(fn($v) => $v * 3 + 1, $values);
$copy = $mapped;
$copy[0] = 99;
echo $mapped[0], "|", $copy[0], "\n";
"#;
    assert_eq!(compile_and_run(source), "4|99\n");
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map ").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map_affine"), "{main}");
}

/// Consumers that may assume boxed Mixed slots keep the generic map for affine callbacks.
#[test]
fn test_stored_numeric_map_affine_unsafe_consumers_keep_generic_map() {
    let source = r#"<?php
function take(array $items): int { return count($items); }
$values = range(1, 3);
$mapped = array_map(fn($v) => $v * 3 + 1, $values);
$second = array_map(fn($v) => $v * 4, $values);
$merged = array_merge($mapped, $second);
$pushed = array_map(fn($v) => $v * 3 + 1, $values);
array_push($pushed, 10);
$called = array_map(fn($v) => $v * 3 + 1, $values);
echo implode(",", $merged), "|", implode(",", $pushed), "|", take($called), "\n";
"#;
    assert_eq!(compile_and_run(source), "4,7,10,4,8,12|4,7,10,10|3\n");
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map ").count(), 4, "{main}");
    assert!(!main.contains("runtime.array_map_affine"), "{main}");
}

/// The safe and promoted affine results release without leaks.
#[test]
fn test_stored_numeric_map_affine_heap_debug_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = range(1, 5);
$mapped = array_map(fn($v) => $v * 3 + 1, $values);
$big = [PHP_INT_MAX];
$overflow = array_map(fn($v) => $v * 2 + 1, $big);
echo array_sum($mapped), "|", gettype($overflow[0]), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "50|double\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// The affine kernel produces the same output with the EIR optimizer on and off.
#[test]
fn test_stored_numeric_map_affine_ir_opt_parity() {
    let source = r#"<?php
$values = range(1, 100);
$sum = 0;
for ($i = 0; $i < 3; $i++) {
    $mapped = array_map(fn($v) => $v * 3 + 1, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "45750\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "45750\n");
}

/// A square callback maps negatives correctly and feeds every neutral consumer.
#[test]
fn test_stored_numeric_map_square_shapes_and_consumers() {
    let source = r#"<?php
$values = [-3, -2, 0, 2, 3];
$mapped = array_map(fn($v) => $v * $v, $values);
echo implode(",", $mapped), "|", array_sum($mapped), "|";
foreach ($mapped as $v) { echo $v, ","; }
echo "\n";
"#;
    assert_eq!(compile_and_run(source), "9,4,0,4,9|26|9,4,0,4,9,\n");
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_square").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// A square overflow promotes only the overflowing elements to float.
#[test]
fn test_stored_numeric_map_square_overflow_preserves_php_types() {
    let source = r#"<?php
$big = [PHP_INT_MAX - 1, PHP_INT_MAX, 2];
$mapped = array_map(fn($v) => $v * $v, $big);
foreach ($mapped as $v) { echo gettype($v), ":", $v, "|"; }
echo "\n";
$min = [PHP_INT_MIN];
$negative = array_map(fn($v) => $v * $v, $min);
foreach ($negative as $v) { echo gettype($v), ":", $v, "|"; }
echo "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "double:8.5070591730235E+37|double:8.5070591730235E+37|integer:4|\n\
         double:8.5070591730235E+37|\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_square").count(), 2, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// Element writes keep the generic boxed map for square callbacks.
#[test]
fn test_stored_numeric_map_square_element_writes_keep_generic_map() {
    let source = r#"<?php
$values = [1, 2, 3];
$mapped = array_map(fn($v) => $v * $v, $values);
$copy = $mapped;
$copy[0] = 99;
echo $mapped[0], "|", $copy[0], "\n";
"#;
    assert_eq!(compile_and_run(source), "1|99\n");
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map ").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map_square"), "{main}");
}

/// The safe and promoted square results release without leaks.
#[test]
fn test_stored_numeric_map_square_heap_debug_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = [1, 2, 3, 4];
$mapped = array_map(fn($v) => $v * $v, $values);
$big = [PHP_INT_MAX];
$overflow = array_map(fn($v) => $v * $v, $big);
echo array_sum($mapped), "|", gettype($overflow[0]), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "30|double\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// The square kernel produces the same output with the EIR optimizer on and off.
#[test]
fn test_stored_numeric_map_square_ir_opt_parity() {
    let source = r#"<?php
$values = range(1, 100);
$sum = 0;
for ($i = 0; $i < 3; $i++) {
    $mapped = array_map(fn($v) => $v * $v, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "1015050\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "1015050\n");
}

/// Square-then-offset callbacks accept both addition orders and subtraction.
#[test]
fn test_stored_numeric_map_square_affine_shapes() {
    let source = r#"<?php
$values = [-2, -1, 0, 1, 2];
$a = array_map(fn($v) => $v * $v + 1, $values);
$b = array_map(fn($v) => 1 + $v * $v, $values);
$c = array_map(fn($v) => $v * $v - 1, $values);
echo implode(",", $a), "|", implode(",", $b), "|", implode(",", $c), "\n";
"#;
    assert_eq!(compile_and_run(source), "5,2,1,2,5|5,2,1,2,5|3,0,-1,0,3\n");
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_square_affine").count(), 3, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// The square and the offset addition promote independently, preserving PHP's types.
#[test]
fn test_stored_numeric_map_square_affine_overflow_preserves_php_types() {
    let source = r#"<?php
$big = [PHP_INT_MAX - 1, PHP_INT_MAX, 2];
$mul = array_map(fn($v) => $v * $v + 1, $big);
foreach ($mul as $v) { echo gettype($v), ":", $v, "|"; }
echo "\n";
$edge = [2];
$add = array_map(fn($v) => $v * $v + PHP_INT_MAX, $edge);
foreach ($add as $v) { echo gettype($v), ":", $v, "|"; }
echo "\n";
$sub = array_map(fn($v) => $v * $v - 2, $edge);
foreach ($sub as $v) { echo gettype($v), ":", $v, "|"; }
echo "\n";
$min = [PHP_INT_MIN];
$negative = array_map(fn($v) => $v * $v + 1, $min);
foreach ($negative as $v) { echo gettype($v), ":", $v, "|"; }
echo "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "double:8.5070591730235E+37|double:8.5070591730235E+37|integer:5|\n\
         double:9.2233720368548E+18|\n\
         integer:2|\n\
         double:8.5070591730235E+37|\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_square_affine").count(), 4, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// Element writes keep the generic boxed map for square-then-offset callbacks.
#[test]
fn test_stored_numeric_map_square_affine_element_writes_keep_generic_map() {
    let source = r#"<?php
$values = [1, 2, 3];
$mapped = array_map(fn($v) => $v * $v + 1, $values);
$copy = $mapped;
$copy[0] = 99;
echo $mapped[0], "|", $copy[0], "\n";
"#;
    assert_eq!(compile_and_run(source), "2|99\n");
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map ").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map_square_affine"), "{main}");
}

/// The safe and promoted square-then-offset results release without leaks.
#[test]
fn test_stored_numeric_map_square_affine_heap_debug_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = [1, 2, 3, 4];
$mapped = array_map(fn($v) => $v * $v + 1, $values);
$big = [PHP_INT_MAX];
$overflow = array_map(fn($v) => $v * $v + 1, $big);
echo array_sum($mapped), "|", gettype($overflow[0]), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "34|double\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// The square-then-offset kernel produces the same output with the EIR optimizer on and off.
#[test]
fn test_stored_numeric_map_square_affine_ir_opt_parity() {
    let source = r#"<?php
$values = range(1, 100);
$sum = 0;
for ($i = 0; $i < 3; $i++) {
    $mapped = array_map(fn($v) => $v * $v + 1, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "1015350\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "1015350\n");
}

/// A float-literal scale maps every int element to a raw double and feeds neutral consumers.
#[test]
fn test_stored_numeric_map_float_scale_shapes_and_consumers() {
    let source = r#"<?php
$values = [1, 2, 3, 4];
$a = array_map(fn($v) => $v * 1.5, $values);
$b = array_map(fn($v) => 2.5 * $v, $values);
$c = array_map(fn($v) => $v * -0.5, $values);
echo implode(",", $a), "|", implode(",", $b), "|", implode(",", $c), "|", array_sum($a), "|";
foreach ($a as $v) { echo gettype($v), ":", $v, ","; }
echo "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "1.5,3,4.5,6|2.5,5,7.5,10|-0.5,-1,-1.5,-2|15|double:1.5,double:3,double:4.5,double:6,\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_float_scale").count(), 3, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// Large integers and identity scales stay in the float domain, matching PHP's conversion.
#[test]
fn test_stored_numeric_map_float_scale_large_and_identity_scales() {
    let source = r#"<?php
$big = [PHP_INT_MAX, PHP_INT_MIN, 0];
$mapped = array_map(fn($v) => $v * 1.5, $big);
foreach ($mapped as $v) { echo gettype($v), ":", $v, "|"; }
echo "\n";
$small = [1, 2, 3];
$one = array_map(fn($v) => $v * 1.0, $small);
echo implode(",", $one), "|", gettype($one[0]), "|";
$zero = array_map(fn($v) => $v * 0.0, $small);
echo implode(",", $zero), "|", gettype($zero[0]), "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "double:1.3835058055282E+19|double:-1.3835058055282E+19|double:0|\n\
         1,2,3|double|0,0,0|double\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_float_scale").count(), 3, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// JSON, direct reads and the boxed aggregate read the raw float slots exactly.
#[test]
fn test_stored_numeric_map_float_scale_json_and_direct_reads() {
    let source = r#"<?php
$values = range(1, 3);
$mapped = array_map(fn($v) => $v * 1.5, $values);
echo json_encode($mapped), "|", count($mapped), "|", array_sum($mapped), "|";
var_dump($mapped[1]);
"#;
    assert_eq!(
        compile_and_run(source),
        "[1.5,3,4.5]|3|9|float(3)\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_float_scale").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// Element writes keep the generic boxed map for float-scale callbacks; the generic untyped
/// float-callback path now boxes the closure's raw F64 return correctly.
#[test]
fn test_stored_numeric_map_float_scale_element_writes_keep_generic_map() {
    let source = r#"<?php
$values = [1, 2, 3];
$mapped = array_map(fn($v) => $v * 1.5, $values);
$copy = $mapped;
$copy[0] = 9.5;
echo $mapped[0], "|", $copy[0], "\n";
"#;
    assert_eq!(compile_and_run(source), "1.5|9.5\n");
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map ").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map_float_scale"), "{main}");
}

/// The raw float result releases without leaks.
#[test]
fn test_stored_numeric_map_float_scale_heap_debug_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = range(1, 5);
$mapped = array_map(fn($v) => $v * 1.5, $values);
echo array_sum($mapped), "|", implode(",", $mapped), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "22.5|1.5,3,4.5,6,7.5\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// The float-scale kernel produces the same output with the EIR optimizer on and off.
#[test]
fn test_stored_numeric_map_float_scale_ir_opt_parity() {
    let source = r#"<?php
$values = range(1, 100);
$sum = 0;
for ($i = 0; $i < 3; $i++) {
    $mapped = array_map(fn($v) => $v * 1.5, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "22725\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "22725\n");
}

/// Float-affine callbacks accept every operand order and feed the raw float consumers.
#[test]
fn test_stored_numeric_map_float_affine_shapes_and_consumers() {
    let source = r#"<?php
$values = [1, 2, 3];
$a = array_map(fn($v) => $v * 1.5 + 2.5, $values);
$b = array_map(fn($v) => 2.5 + $v * 1.5, $values);
$c = array_map(fn($v) => $v * 1.5 - 2.5, $values);
$d = array_map(fn($v) => 2.5 - $v * 1.5, $values);
$e = array_map(fn($v) => $v * 2 + 1.5, $values);
$f = array_map(fn($v) => $v * 1.5 + 1, $values);
echo implode(",", $a), "|", implode(",", $b), "|", implode(",", $c), "|", implode(",", $d), "|", implode(",", $e), "|", implode(",", $f), "\n";
echo array_sum($a), "|", json_encode($a), "|";
foreach ($a as $v) { echo gettype($v), ":", $v, ","; }
echo "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "4,5.5,7|4,5.5,7|-1,0.5,2|1,-0.5,-2|3.5,5.5,7.5|2.5,4,5.5\n\
         16.5|[4,5.5,7]|double:4,double:5.5,double:7,\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_float_affine").count(), 6, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// Large integers convert to double before the scale and offset are applied.
#[test]
fn test_stored_numeric_map_float_affine_large_values() {
    let source = r#"<?php
$big = [PHP_INT_MAX, PHP_INT_MIN, 0];
$mapped = array_map(fn($v) => $v * 1.5 + 2.5, $big);
foreach ($mapped as $v) { echo gettype($v), ":", $v, "|"; }
echo "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "double:1.3835058055282E+19|double:-1.3835058055282E+19|double:2.5|\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_float_affine").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// Element writes keep the generic boxed map for float-affine callbacks and still run.
#[test]
fn test_stored_numeric_map_float_affine_element_writes_keep_generic_map() {
    let source = r#"<?php
$values = [1, 2, 3];
$mapped = array_map(fn($v) => $v * 1.5 + 2.5, $values);
$copy = $mapped;
$copy[0] = 9.5;
echo $mapped[0], "|", $copy[0], "\n";
"#;
    assert_eq!(compile_and_run(source), "4|9.5\n");
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map ").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map_float_affine"), "{main}");
}

/// The raw float-affine result releases without leaks.
#[test]
fn test_stored_numeric_map_float_affine_heap_debug_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = range(1, 5);
$mapped = array_map(fn($v) => $v * 1.5 + 2.5, $values);
echo array_sum($mapped), "|", implode(",", $mapped), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "35|4,5.5,7,8.5,10\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// The float-affine kernel produces the same output with the EIR optimizer on and off.
#[test]
fn test_stored_numeric_map_float_affine_ir_opt_parity() {
    let source = r#"<?php
$values = range(1, 100);
$sum = 0;
for ($i = 0; $i < 3; $i++) {
    $mapped = array_map(fn($v) => $v * 1.5 + 2.5, $values);
    $sum += array_sum($mapped);
}
echo $sum, "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "23475\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "23475\n");
}

/// Float addition and subtraction reuse the float-affine kernel with a unit scale.
#[test]
fn test_stored_numeric_map_float_add_and_sub() {
    let source = r#"<?php
$values = [-2, -1, 0, 1, 2];
$a = array_map(fn($v) => $v + 1.5, $values);
$b = array_map(fn($v) => 1.5 + $v, $values);
$c = array_map(fn($v) => $v - 1.5, $values);
$d = array_map(fn($v) => $v + -0.0, $values);
echo implode(",", $a), "|", implode(",", $b), "|", implode(",", $c), "|", implode(",", $d), "|";
foreach ($a as $v) { echo gettype($v), ":", $v, ","; }
$empty = array_slice($values, 0, 0);
$e = array_map(fn($v) => $v + 1.5, $empty);
echo "|", count($e), "|", json_encode($e), "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "-0.5,0.5,1.5,2.5,3.5|-0.5,0.5,1.5,2.5,3.5|-3.5,-2.5,-1.5,-0.5,0.5|\
         -2,-1,0,1,2|double:-0.5,double:0.5,double:1.5,double:2.5,double:3.5,|0|[]\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_float_affine").count(), 5, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// A nonzero float divisor uses the raw double division kernel; zero and int divisors keep the
/// generic map so PHP's DivisionByZeroError and int/float result distinction survive.
#[test]
fn test_stored_numeric_map_float_div() {
    let source = r#"<?php
$values = [1, 2, 3, 4];
$a = array_map(fn($v) => $v / 2.0, $values);
$b = array_map(fn($v) => $v / -2.5, $values);
$c = array_map(fn($v) => $v / INF, [-2, 1]);
$d = array_map(fn($v) => $v / 2, [1, 2, 3]);
echo implode(",", $a), "|", implode(",", $b), "|", implode(",", $c), "|", implode(",", $d), "|";
foreach ($a as $v) { echo gettype($v), ":", $v, ","; }
echo "|", json_encode($a), "\n";
try {
    array_map(fn($v) => $v / 0.0, $values);
} catch (DivisionByZeroError $e) {
    echo "zero-division\n";
}
try {
    array_map(fn($v) => $v / -0.0, $values);
} catch (DivisionByZeroError $e) {
    echo "negzero-division\n";
}
"#;
    assert_eq!(
        compile_and_run(source),
        "0.5,1,1.5,2|-0.4,-0.8,-1.2,-1.6|-0,0|0.5,1,1.5|\
         double:0.5,double:1,double:1.5,double:2,|[0.5,1,1.5,2]\n\
         zero-division\n\
         negzero-division\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_float_div").count(), 3, "{main}");
    // The integer-divisor map now uses the exact int-division kernel; both zero-divisor
    // maps stay generic.
    assert_eq!(main.matches("runtime.array_map_int_div").count(), 1, "{main}");
    assert_eq!(main.matches("runtime.array_map ").count(), 2, "{main}");
}

/// Non-empty all-int array literals are proven packed `array<int>` sources and use the raw
/// kernels instead of the generic boxed map.
#[test]
fn test_stored_numeric_map_array_literal_sources() {
    let source = r#"<?php
$values = [-2, 0, 3];
$scale = array_map(fn($v) => $v * 1.5, [1, 2, 3]);
$add = array_map(fn($v) => $v + 1.5, $values);
$div = array_map(fn($v) => $v / 2.0, [2, 4, 5]);
$square = array_map(fn($v) => $v * $v, [1, 2, 3]);
$intadd = array_map(fn($v) => $v + 1, [1, 2, 3]);
echo implode(",", $scale), "|", implode(",", $add), "|", implode(",", $div), "|", implode(",", $square), "|", implode(",", $intadd), "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "1.5,3,4.5|-0.5,1.5,4.5|1,2,2.5|1,4,9|2,3,4\n"
    );
    let main = emit_main_ir(source, true);
    for target in [
        "runtime.array_map_float_scale",
        "runtime.array_map_float_affine",
        "runtime.array_map_float_div",
        "runtime.array_map_square",
        "runtime.array_map_numeric",
    ] {
        assert_eq!(main.matches(target).count(), 1, "{target}: {main}");
    }
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// Element writes on a literal-source result keep the generic map and still run.
#[test]
fn test_stored_numeric_map_array_literal_element_write_keeps_generic() {
    let source = r#"<?php
$mapped = array_map(fn($v) => $v + 1.5, [1, 2, 3]);
$mapped[0] = 9.5;
echo implode(",", $mapped), "\n";
"#;
    assert_eq!(compile_and_run(source), "9.5,3.5,4.5\n");
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map ").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map_float_affine"), "{main}");
}

/// The new float shapes and literal sources produce the same output with the EIR optimizer on
/// and off.
#[test]
fn test_stored_numeric_map_float_shapes_ir_opt_parity() {
    let source = r#"<?php
$values = range(1, 100);
$sum = 0;
for ($i = 0; $i < 3; $i++) {
    $add = array_map(fn($v) => $v + 1.5, $values);
    $div = array_map(fn($v) => $v / 2.0, $values);
    $lit = array_map(fn($v) => $v * 2.5, [1, 2, 3]);
    $sum += array_sum($add) + array_sum($div) + array_sum($lit);
}
echo $sum, "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "23220\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "23220\n");
}

/// Literal-source results release their temporary source and result arrays cleanly.
#[test]
fn test_stored_numeric_map_float_literals_heap_debug_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = [-2, 0, 3];
$scale = array_map(fn($v) => $v * 1.5, [1, 2, 3]);
$add = array_map(fn($v) => $v + 1.5, $values);
$div = array_map(fn($v) => $v / 2.0, [2, 4, 5]);
$square = array_map(fn($v) => $v * $v, [1, 2, 3]);
$intadd = array_map(fn($v) => $v + 1, [1, 2, 3]);
echo implode(",", $scale), "|", implode(",", $add), "|", implode(",", $div), "|", implode(",", $square), "|", implode(",", $intadd), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "1.5,3,4.5|-0.5,1.5,4.5|1,2,2.5|1,4,9|2,3,4\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Emits target assembly for one fixture, using staticlib output for every target.
fn emit_target_assembly(source: &str, target: &str) -> String {
    let dir = make_cli_test_dir("elephc_stored_float_map_targets");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write stored float map target fixture");
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
        .expect("read stored float map target assembly");
    let _ = fs::remove_dir_all(dir);
    assembly
}

/// Every supported target materializes the float add/div/scale calls and the literal source.
#[test]
fn test_stored_numeric_map_float_shapes_emit_all_supported_targets() {
    let source = r#"<?php
#[Export]
function floatShapes(int $seed): float {
    $add = array_map(fn($v) => $v + 1.5, [1, 2, 3]);
    $div = array_map(fn($v) => $v / 2.0, [1, 2, 3]);
    $lit = array_map(fn($v) => $v * 1.5, [1, 2, 3]);
    return array_sum($add) + array_sum($div) + array_sum($lit) + $seed;
}
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let assembly = emit_target_assembly(source, target);
        assert!(assembly.contains("__rt_array_map_float_affine"), "{target}");
        assert!(assembly.contains("__rt_array_map_float_div"), "{target}");
        assert!(assembly.contains("__rt_array_map_float_scale"), "{target}");
        assert!(!assembly.contains("__rt_array_map_boxed"), "{target}");
    }
}

/// Function-local `array<int>` sources inferred by the lowering context fuse like main-level
/// ones, including `range` locals, reassignments, merged branches, and stepped ranges.
#[test]
fn test_stored_numeric_map_function_local_sources() {
    let source = r#"<?php
function mappedRange(): float {
    $values = range(1, 10);
    $mapped = array_map(fn($v) => $v * 1.5, $values);
    return array_sum($mapped);
}
function mappedLiteral(): float {
    $values = [1, 2, 3];
    $mapped = array_map(fn($v) => $v + 1.5, $values);
    return array_sum($mapped);
}
function mappedDiv(): float {
    $values = [2, 4, 5];
    $mapped = array_map(fn($v) => $v / 2.0, $values);
    return array_sum($mapped);
}
function reassigned(): float {
    $values = [1, 2, 3];
    $values = [4, 5];
    $mapped = array_map(fn($v) => $v + 1.5, $values);
    return array_sum($mapped);
}
function branchInt(int $flag): float {
    if ($flag) {
        $values = [1, 2, 3];
    } else {
        $values = [4, 5];
    }
    $mapped = array_map(fn($v) => $v + 1.5, $values);
    return array_sum($mapped);
}
function stepped(): float {
    $values = range(1, 10, 2);
    $mapped = array_map(fn($v) => $v * 1.5, $values);
    return array_sum($mapped);
}
echo mappedRange(), "|", mappedLiteral(), "|", mappedDiv(), "|", reassigned(), "|";
echo branchInt(1), "|", branchInt(0), "|", stepped(), "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "82.5|10.5|5.5|12|10.5|12|37.5\n"
    );
    let eir = emit_ir(source, true);
    assert!(eir.contains("runtime.array_map_float_scale"), "{eir}");
    assert!(eir.contains("runtime.array_map_float_affine"), "{eir}");
    assert!(eir.contains("runtime.array_map_float_div"), "{eir}");
    assert!(!eir.contains("runtime.array_map "), "{eir}");
}

/// A function-local source whose flow type is not `array<int>` falls back to the generic map
/// instead of trusting the marker's shape scan.
#[test]
fn test_stored_numeric_map_function_local_mixed_source_falls_back() {
    let source = r#"<?php
function mixedSource(array $values): float {
    $mapped = array_map(fn($v) => $v + 1.5, $values);
    return array_sum($mapped);
}
echo mixedSource([1, 2, 3]), "\n";
"#;
    assert_eq!(compile_and_run(source), "10.5\n");
    let eir = emit_ir(source, true);
    assert_eq!(eir.matches("runtime.array_map ").count(), 1, "{eir}");
    assert!(!eir.contains("runtime.array_map_float_affine"), "{eir}");
}

/// A reference alias disqualifies the source before lowering, so the generic map runs.
#[test]
fn test_stored_numeric_map_function_local_alias_falls_back() {
    let source = r#"<?php
$values = [1, 2, 3];
$alias =& $values;
$mapped = array_map(fn($v) => $v + 1, $values);
echo implode(",", $mapped), "\n";
"#;
    assert_eq!(compile_and_run(source), "2,3,4\n");
    let eir = emit_ir(source, true);
    assert_eq!(eir.matches("runtime.array_map ").count(), 1, "{eir}");
    assert!(!eir.contains("runtime.array_map_numeric"), "{eir}");
}

/// Function-local fused maps release their sources and results cleanly.
#[test]
fn test_stored_numeric_map_function_local_heap_debug_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function mapped(): float {
    $values = range(1, 10);
    $add = array_map(fn($v) => $v + 1.5, $values);
    $div = array_map(fn($v) => $v / 2.0, $values);
    return array_sum($add) + array_sum($div);
}
echo mapped(), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "97.5\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Function-local fused maps produce the same output with the EIR optimizer on and off.
#[test]
fn test_stored_numeric_map_function_local_ir_opt_parity() {
    let source = r#"<?php
function mapped(int $n): float {
    $values = range(1, $n);
    $add = array_map(fn($v) => $v + 1.5, $values);
    $div = array_map(fn($v) => $v / 2.0, $values);
    return array_sum($add) + array_sum($div);
}
echo mapped(10), "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "97.5\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "97.5\n");
}

/// Every supported target emits the fused kernels for a function-local source.
#[test]
fn test_stored_numeric_map_function_local_emits_all_supported_targets() {
    let source = r#"<?php
#[Export]
function localFloat(int $seed): float {
    $values = range(1, 10);
    $add = array_map(fn($v) => $v + 1.5, $values);
    $div = array_map(fn($v) => $v / 2.0, $values);
    $scale = array_map(fn($v) => $v * 1.5, $values);
    return array_sum($add) + array_sum($div) + array_sum($scale) + $seed;
}
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let assembly = emit_target_assembly(source, target);
        assert!(assembly.contains("__rt_array_map_float_affine"), "{target}");
        assert!(assembly.contains("__rt_array_map_float_div"), "{target}");
        assert!(assembly.contains("__rt_array_map_float_scale"), "{target}");
        assert!(!assembly.contains("__rt_array_map_boxed"), "{target}");
    }
}

/// Direct side-effect-free sources: `range()` with int literals, `array_slice()` over a
/// proven packed source, nested slices, and slices of array literals.
#[test]
fn test_stored_numeric_map_direct_packed_sources() {
    let source = r#"<?php
$a = array_map(fn($v) => $v + 1.5, range(1, 5));
$b = array_map(fn($v) => $v * 1.5, range(1, 10, 2));
$c = array_map(fn($v) => $v / 2.0, array_slice(range(1, 10), 2, 3));
$d = array_map(fn($v) => $v + 1, range(1, 5));
$e = array_map(fn($v) => $v + 1.5, array_slice(range(1, 10), 2, 3, false));
$f = array_map(fn($v) => $v * 1.5, array_slice([1, 2, 3, 4], 1));
echo implode(",", $a), "|", implode(",", $b), "|", implode(",", $c), "|";
echo implode(",", $d), "|", implode(",", $e), "|", implode(",", $f), "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "2.5,3.5,4.5,5.5,6.5|1.5,4.5,7.5,10.5,13.5|1.5,2,2.5|\
         2,3,4,5,6|4.5,5.5,6.5|3,4.5,6\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_float_affine").count(), 2, "{main}");
    assert_eq!(main.matches("runtime.array_map_float_scale").count(), 2, "{main}");
    assert_eq!(main.matches("runtime.array_map_float_div").count(), 1, "{main}");
    assert_eq!(main.matches("runtime.array_map_numeric").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// A direct slice over a variable and nested slices keep the packed-int proof.
#[test]
fn test_stored_numeric_map_direct_slice_over_variable() {
    let source = r#"<?php
$values = range(1, 10);
$mapped = array_map(fn($v) => $v + 1.5, array_slice($values, 2, 3));
$nested = array_map(fn($v) => $v * 1.5, array_slice(array_slice($values, 1), 1, 4));
echo implode(",", $mapped), "|", implode(",", $nested), "\n";
"#;
    assert_eq!(compile_and_run(source), "4.5,5.5,6.5|4.5,6,7.5,9\n");
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_float_affine").count(), 1, "{main}");
    assert_eq!(main.matches("runtime.array_map_float_scale").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// `array_values()` over a packed list aliases the same indexed layout, so it remains a
/// direct source for the stored-map kernels.
#[test]
fn test_stored_numeric_map_array_values_source() {
    let source = r#"<?php
$values = range(1, 10);
$a = array_map(fn($v) => $v + 1, array_values($values));
$b = array_map(fn($v) => $v * 1.5, array_values(array_slice($values, 2, 3)));
$c = array_map(fn($v) => $v / 2.0, array_values([1, 2, 3, 4]));
echo implode(",", $a), "|", implode(",", $b), "|", implode(",", $c), "
";
"#;
    assert_eq!(
        compile_and_run(source),
        "2,3,4,5,6,7,8,9,10,11|4.5,6,7.5|0.5,1,1.5,2
"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_numeric").count(), 1, "{main}");
    assert_eq!(main.matches("runtime.array_map_float_scale").count(), 1, "{main}");
    assert_eq!(main.matches("runtime.array_map_float_div").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// `array_slice(..., 0, ..., true)` preserves the 0..n-1 keys of a packed list, so it
/// remains a direct packed source; a later offset still falls back to the generic map.
#[test]
fn test_stored_numeric_map_preserve_keys_at_zero_offset() {
    let source = r#"<?php
$values = range(1, 10);
$a = array_map(fn($v) => $v + 1, array_slice($values, 0, 4, true));
$b = array_map(fn($v) => $v * 1.5, array_slice($values, 0, null, true));
$c = array_map(fn($v) => $v / 2.0, array_slice(array_slice($values, 0, 6, true), 0, 3, true));
$d = array_map(fn($v) => $v * 2, array_slice(range(1, 5), 0, 3, true));
echo implode(",", $a), "|", implode(",", $b), "|", implode(",", $c), "|", implode(",", $d), "
";
"#;
    assert_eq!(
        compile_and_run(source),
        "2,3,4,5|1.5,3,4.5,6,7.5,9,10.5,12,13.5,15|0.5,1,1.5|2,4,6
"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_numeric").count(), 2, "{main}");
    assert_eq!(main.matches("runtime.array_map_float_scale").count(), 1, "{main}");
    assert_eq!(main.matches("runtime.array_map_float_div").count(), 1, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// A boxed parameter or global source keeps the generic map: its key-preserving slice lowers
/// to a hash, so the packed-source proof must fail before any emission.
#[test]
fn test_stored_numeric_map_preserve_keys_boxed_binding_falls_back() {
    let source = r#"<?php
$values = [1, 2, 3, 4];
function mapParameter($values) {
    return array_map(fn($v) => $v + 1, array_slice($values, 0, 4, true));
}
function mapGlobal() {
    global $values;
    return array_map(fn($v) => $v + 1, array_slice($values, 0, 4, true));
}
echo implode(",", mapParameter([1, 2, 3, 4])), "|", implode(",", mapGlobal()), "\n";
"#;
    assert_eq!(compile_and_run(source), "2,3,4,5|2,3,4,5\n");
    let module = emit_ir(source, true);
    assert_eq!(module.matches("runtime.array_map ").count(), 2, "{module}");
    assert!(!module.contains("runtime.array_map_numeric"), "{module}");
}

/// Unproven direct sources keep the generic map and are evaluated exactly once.
#[test]
fn test_stored_numeric_map_direct_sources_fall_back() {
    let source = r#"<?php
$start = $argc + 1;
$dynamic = array_map(fn($v) => $v + 1.5, range($start, 5));
echo implode(",", $dynamic), "|";
$mixed = ["a", "b", "c"];
$i = 0;
$sliced = array_map(fn($v) => $v . "!", array_slice($mixed, $i++));
echo implode(",", $sliced), "|", $i, "|";
$values = range(1, 5);
$keys = array_map(fn($v) => $v + 1.5, array_slice($values, 2, null, true));
echo implode(",", $keys), "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "3.5,4.5,5.5,6.5|a!,b!,c!|1|4.5,5.5,6.5\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map ").count(), 3, "{main}");
    assert!(!main.contains("runtime.array_map_float_affine"), "{main}");
    assert!(!main.contains("runtime.array_map_numeric"), "{main}");
}

/// Direct source temporaries and result arrays release cleanly.
#[test]
fn test_stored_numeric_map_direct_sources_heap_debug_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = range(1, 10);
$a = array_map(fn($v) => $v + 1.5, range(1, 5));
$b = array_map(fn($v) => $v * 1.5, array_slice($values, 2, 3));
$c = array_map(fn($v) => $v / 2.0, array_slice(range(1, 10), 1, 4));
$d = array_map(fn($v) => $v + 1, array_slice([1, 2, 3, 4], 1));
$e = array_map(fn($v) => $v + 1, array_values(array_slice($values, 2, 3)));
$f = array_map(fn($v) => $v + 1, array_slice($values, 0, 4, true));
echo implode(",", $a), "|", implode(",", $b), "|", implode(",", $c), "|", implode(",", $d), "|", implode(",", $e), "|", implode(",", $f), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "2.5,3.5,4.5,5.5,6.5|4.5,6,7.5|1,1.5,2,2.5|3,4,5|4,5,6|2,3,4,5\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Direct fused sources produce the same output with the EIR optimizer on and off.
#[test]
fn test_stored_numeric_map_direct_sources_ir_opt_parity() {
    let source = r#"<?php
$sum = 0;
for ($i = 0; $i < 3; $i++) {
    $a = array_map(fn($v) => $v + 1.5, range(1, 10));
    $b = array_map(fn($v) => $v / 2.0, array_slice(range(1, 10), 2, 3));
    $c = array_map(fn($v) => $v + 1, array_slice(range(1, 10), 0, 4, true));
    $sum += array_sum($a) + array_sum($b) + array_sum($c);
}
echo $sum, "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "270\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "270\n");
}

/// Every supported target emits the fused kernels for direct range/slice sources.
#[test]
fn test_stored_numeric_map_direct_sources_emit_all_supported_targets() {
    let source = r#"<?php
#[Export]
function directSources(int $seed): float {
    $a = array_map(fn($v) => $v + 1.5, range(1, 10));
    $b = array_map(fn($v) => $v / 2.0, array_slice(range(1, 10), 2, 3));
    $c = array_map(fn($v) => $v + 1, array_values(range(1, 5)));
    $d = array_map(fn($v) => $v * 1.5, array_slice(range(1, 10), 0, 4, true));
    return array_sum($a) + array_sum($b) + array_sum($c) + array_sum($d) + $seed;
}
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let assembly = emit_target_assembly(source, target);
        assert!(assembly.contains("__rt_array_map_float_affine"), "{target}");
        assert!(assembly.contains("__rt_array_map_float_div"), "{target}");
        assert!(assembly.contains("__rt_array_map_numeric"), "{target}");
        assert!(!assembly.contains("__rt_array_map_boxed"), "{target}");
    }
}

/// An integer divisor keeps a raw `array<int>` while every quotient is exact and rebuilds a
/// Mixed array at the first inexact element.
#[test]
fn test_stored_numeric_map_int_div_exact_and_mixed() {
    let source = r#"<?php
$values = [1, 2, 3, 4, 5];
$mapped = array_map(fn($v) => $v / 2, $values);
foreach ($mapped as $v) { var_dump($v); }
$exact = array_map(fn($v) => $v / 2, [2, 4, 6]);
foreach ($exact as $v) { var_dump($v); }
echo implode(",", $mapped), "|", implode(",", $exact), "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "float(0.5)\nint(1)\nfloat(1.5)\nint(2)\nfloat(2.5)\n\
         int(1)\nint(2)\nint(3)\n\
         0.5,1,1.5,2,2.5|1,2,3\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_int_div").count(), 2, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// Negative divisors, INT_MIN / -1, INT_MIN / 1 and INT_MIN / -2 follow PHP exactly.
#[test]
fn test_stored_numeric_map_int_div_edge_divisors() {
    let source = r#"<?php
$min = [PHP_INT_MIN];
$minusOne = array_map(fn($v) => $v / -1, $min);
var_dump($minusOne[0]);
$one = array_map(fn($v) => $v / 1, $min);
var_dump($one[0]);
$minusTwo = array_map(fn($v) => $v / -2, $min);
var_dump($minusTwo[0]);
$neg = array_map(fn($v) => $v / -2, [5, 6, -7]);
foreach ($neg as $v) { var_dump($v); }
"#;
    assert_eq!(
        compile_and_run(source),
        "float(9.223372036854776E+18)\n\
         int(-9223372036854775808)\n\
         int(4611686018427387904)\n\
         float(-2.5)\nint(-3)\nfloat(3.5)\n"
    );
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_int_div").count(), 4, "{main}");
    assert!(!main.contains("runtime.array_map "), "{main}");
}

/// Zero divisors keep the generic map and its DivisionByZeroError; float divisors keep the
/// raw float kernel; an empty source stays generic.
#[test]
fn test_stored_numeric_map_int_div_fallbacks() {
    let source = r#"<?php
$values = [1, 2, 3];
try {
    array_map(fn($v) => $v / 0, $values);
} catch (DivisionByZeroError $e) {
    echo "zero|";
}
$float = array_map(fn($v) => $v / 2.0, $values);
echo implode(",", $float), "|";
$empty = array_map(fn($v) => $v / 3, []);
echo count($empty), "\n";
"#;
    assert_eq!(compile_and_run(source), "zero|0.5,1,1.5|0\n");
    let main = emit_main_ir(source, true);
    assert_eq!(main.matches("runtime.array_map_float_div").count(), 1, "{main}");
    assert_eq!(main.matches("runtime.array_map ").count(), 2, "{main}");
    assert!(!main.contains("runtime.array_map_int_div"), "{main}");
}

/// The raw and Mixed integer-division results release cleanly.
#[test]
fn test_stored_numeric_map_int_div_heap_debug_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = [1, 2, 3, 4];
$exact = [2, 4, 6];
$mapped = array_map(fn($v) => $v / 2, $values);
$raw = array_map(fn($v) => $v / 2, $exact);
$min = [PHP_INT_MIN];
$overflow = array_map(fn($v) => $v / -1, $min);
echo implode(",", $mapped), "|", implode(",", $raw), "|", gettype($overflow[0]), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "0.5,1,1.5,2|1,2,3|double\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Integer-division maps produce the same output with the EIR optimizer on and off.
#[test]
fn test_stored_numeric_map_int_div_ir_opt_parity() {
    let source = r#"<?php
$sum = 0;
for ($i = 0; $i < 3; $i++) {
    $mixed = array_map(fn($v) => $v / 2, [1, 2, 3, 4]);
    $raw = array_map(fn($v) => $v / 2, [2, 4, 6]);
    $sum += array_sum($mixed) + array_sum($raw);
}
echo $sum, "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "33\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "33\n");
}

/// Every supported target emits the integer-division map kernel.
#[test]
fn test_stored_numeric_map_int_div_emits_all_supported_targets() {
    let source = r#"<?php
#[Export]
function intDivMap(int $seed): int {
    $values = range(2, 20, 2);
    $mapped = array_map(fn($v) => $v / 2, $values);
    return (int)array_sum($mapped) + $seed;
}
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let assembly = emit_target_assembly(source, target);
        assert!(assembly.contains("__rt_array_map_int_div"), "{target}");
        assert!(!assembly.contains("__rt_array_map_boxed"), "{target}");
    }
}

/// Every supported target emits the boxed Mixed division helper when a try handler
/// disables the frame-cell proof.
#[test]
fn test_mixed_division_emits_all_supported_targets() {
    let source = r#"<?php
#[Export]
function mixedDiv(int $seed): int {
    $a = $seed + 5;
    $b = $seed + 2;
    try {
        return (int)($a / $b);
    } catch (DivisionByZeroError $e) {
        return 0;
    }
}
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let assembly = emit_target_assembly(source, target);
        assert!(assembly.contains("__rt_mixed_numeric_div"), "{target}");
    }
}
