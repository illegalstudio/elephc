//! Purpose:
//! End-to-end coverage for the stored `array_map(fn($v) => $v + C | $v - C | $v * C |
//! $v * A + B | $v * $v | $v * $v + C, $ints)` kernels that keep a raw `array<int>`
//! when no element overflows.
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

/// Emits the `main` function body for one fixture in the requested optimizer mode.
fn emit_main_ir(source: &str, ir_opt: bool) -> String {
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
