//! Purpose:
//! End-to-end coverage for the stored `array_map(fn($v) => $v + C | $v - C | $v * C, $ints)`
//! kernel that keeps a raw `array<int>` when no element overflows.
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
