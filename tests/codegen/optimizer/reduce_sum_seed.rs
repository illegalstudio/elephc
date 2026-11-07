//! Purpose:
//! End-to-end coverage for the seeded raw integer `array_reduce()` sum fusion.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - `array_reduce($ints, fn($c, $v) => $c + $v, C)` with a nonzero int `C` (literal or a
//!   local proven `int`) lowers to `ArrayReduceSumSeed`, which keeps the carry raw and boxes
//!   only the final int-or-float.
//! - A float `C` (literal or a local proven `float`) lowers to `ArrayReduceSumFloatSeed`, whose
//!   accumulator starts in double mode.
//! - Initial zero still uses `array_sum()`; non-add bodies and initials whose raw type cannot be
//!   proven before lowering keep the generic reduce path and are evaluated exactly once.
//! - Overflow promotes the integer carry to float exactly like PHP's left-to-right addition.

use crate::support::*;

/// Emits the complete EIR for one fixture.
fn emit_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_reduce_sum_seed_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write reduce seed fixture");
    let output = elephc_cli_command(&dir)
        .arg("--emit-ir")
        .arg("--ir-opt=on")
        .arg(&php_path)
        .output()
        .expect("run elephc --emit-ir");
    assert!(
        output.status.success(),
        "emit-ir failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("EIR is UTF-8");
    let _ = fs::remove_dir_all(dir);
    text
}

/// Emits the optimized `main` function body for one fixture.
fn emit_main_ir(source: &str) -> String {
    emit_ir(source)
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
    let dir = make_cli_test_dir("elephc_reduce_sum_seed_run");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write reduce seed run fixture");
    let compiled = elephc_cli_command(&dir)
        .arg(mode)
        .arg(&php_path)
        .output()
        .expect("compile reduce seed fixture");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = std::process::Command::new(dir.join("main"))
        .output()
        .expect("run reduce seed fixture");
    let _ = fs::remove_dir_all(dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("reduce seed output is UTF-8")
}

/// A nonzero literal initial and both operand orders use the raw seeded reducer.
#[test]
fn test_reduce_sum_seed_uses_raw_accumulator() {
    let source = r#"<?php
$values = [1, 2, 3, 4];
echo array_reduce($values, fn($c, $v) => $c + $v, 10), "|";
echo array_reduce($values, fn($c, $v) => $v + $c, -3), "|";
$empty = array_slice($values, 0, 0);
echo array_reduce($empty, fn($c, $v) => $c + $v, 7), "|";
$big = [PHP_INT_MAX];
$overflow = array_reduce($big, fn($c, $v) => $c + $v, 1);
echo gettype($overflow), ":", $overflow, "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "20|7|7|double:9.2233720368548E+18\n"
    );
    let main = emit_main_ir(source);
    assert_eq!(main.matches("runtime.array_reduce_sum_seed").count(), 4, "{main}");
    assert!(!main.contains("runtime.array_reduce "), "{main}");
}

/// Non-add callback bodies keep the generic boxed reducer.
#[test]
fn test_reduce_sum_seed_keeps_generic_for_other_bodies() {
    let source = r#"<?php
$values = [1, 2, 3];
echo array_reduce($values, fn($c, $v) => $c * $v, 2), "|";
echo array_reduce($values, fn($c, $v) => $c ^ $v, 0), "\n";
"#;
    assert_eq!(compile_and_run(source), "12|0\n");
    let main = emit_main_ir(source);
    assert_eq!(main.matches("runtime.array_reduce ").count(), 2, "{main}");
    assert!(!main.contains("runtime.array_reduce_sum_seed"), "{main}");
}

/// The seeded reducer releases its final boxed result without leaks.
#[test]
fn test_reduce_sum_seed_heap_debug_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = range(1, 5);
$sum = array_reduce($values, fn($c, $v) => $c + $v, 10);
$big = [PHP_INT_MAX];
$overflow = array_reduce($big, fn($c, $v) => $c + $v, 1);
echo $sum, "|", gettype($overflow), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "25|double\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// The seeded reducer produces the same output with the EIR optimizer on and off.
#[test]
fn test_reduce_sum_seed_ir_opt_parity() {
    let source = r#"<?php
$values = range(1, 100);
$sum = 0;
for ($i = 0; $i < 3; $i++) {
    $sum += array_reduce($values, fn($c, $v) => $c + $v, 10);
}
echo $sum, "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "15180\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "15180\n");
}

/// Emits target assembly for one fixture, using staticlib output for every target.
fn emit_target_assembly(source: &str, target: &str) -> String {
    let dir = make_cli_test_dir("elephc_reduce_sum_seed_targets");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write reduce seed target fixture");
    let output = elephc_cli_command(&dir)
        .args(["--emit", "staticlib", "--emit-asm", "--target", target])
        .arg(&php_path)
        .output()
        .expect("run elephc --emit-asm");
    assert!(
        output.status.success(),
        "emit-asm for {target} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let assembly = fs::read_to_string(php_path.with_extension("s"))
        .expect("read reduce seed target assembly");
    let _ = fs::remove_dir_all(dir);
    assembly
}

/// Every supported target emits the seeded reducer call and no generic boxed reduce.
#[test]
fn test_reduce_sum_seed_emits_all_supported_targets() {
    let source = r#"<?php
#[Export]
function reduceSeed(): int {
    $values = range(1, 10);
    return (int)array_reduce($values, fn($c, $v) => $c + $v, 10);
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
        assert!(assembly.contains("__rt_array_sum_int_i64"), "{target}");
        assert!(!assembly.contains("__rt_array_reduce_boxed"), "{target}");
        if target == "linux-x86_64" {
            assert!(assembly.contains("mov rdx, QWORD PTR [rdi]"), "{target}");
        } else {
            assert!(assembly.contains("ldr x2, [x0]"), "{target}");
        }
    }
}

/// Non-literal raw-int seeds (locals and typed parameters) use the raw seeded reducer.
#[test]
fn test_reduce_sum_seed_non_literal_int_seeds() {
    let source = r#"<?php
$values = [1, 2, 3];
$seed = 10;
echo array_reduce($values, fn($c, $v) => $c + $v, $seed), "|";
function reduceWith(int $seed): mixed {
    $items = [1, 2, 3];
    return array_reduce($items, fn($c, $v) => $v + $c, $seed);
}
echo reduceWith(5), "|";
$neg = -7;
echo array_reduce($values, fn($c, $v) => $c + $v, $neg), "|";
$big = [PHP_INT_MAX];
$overflow = array_reduce($big, fn($c, $v) => $c + $v, $neg);
echo gettype($overflow), ":", $overflow, "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "16|11|-1|integer:9223372036854775800\n"
    );
    let eir = emit_ir(source);
    assert_eq!(eir.matches("runtime.array_reduce_sum_seed").count(), 4, "{eir}");
    assert!(!eir.contains("runtime.array_reduce "), "{eir}");
}

/// Float literal and raw-float local/parameter seeds use the raw double reducer.
#[test]
fn test_reduce_sum_seed_float_seeds() {
    let source = r#"<?php
$values = [1, 2, 3];
$literal = array_reduce($values, fn($c, $v) => $c + $v, 1.5);
echo $literal, "|", gettype($literal), "|";
$empty = array_slice($values, 0, 0);
$emptyResult = array_reduce($empty, fn($c, $v) => $c + $v, 1.5);
echo $emptyResult, "|", gettype($emptyResult), "|";
$seed = 2.5;
$variable = array_reduce($values, fn($c, $v) => $c + $v, $seed);
echo $variable, "|", gettype($variable), "|";
$negzero = array_reduce($empty, fn($c, $v) => $c + $v, -0.0);
var_dump($negzero);
$big = [PHP_INT_MAX];
$large = array_reduce($big, fn($c, $v) => $c + $v, 1.5);
echo gettype($large), ":", $large, "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "7.5|double|1.5|double|8.5|double|float(-0)\ndouble:9.2233720368548E+18\n"
    );
    let main = emit_main_ir(source);
    assert_eq!(
        main.matches("runtime.array_reduce_sum_float_seed").count(),
        5,
        "{main}"
    );
    assert!(!main.contains("runtime.array_reduce "), "{main}");
}

/// A float seed received as a typed parameter keeps the raw double representation.
#[test]
fn test_reduce_sum_seed_float_parameter() {
    let source = r#"<?php
function reduceWith(float $seed): float {
    $items = [1, 2, 3];
    return array_reduce($items, fn($c, $v) => $c + $v, $seed);
}
echo reduceWith(2.5), "|", gettype(reduceWith(2.5)), "|";
echo reduceWith(-0.0), "|", gettype(reduceWith(-0.0)), "\n";
"#;
    assert_eq!(compile_and_run(source), "8.5|double|6|double\n");
    let eir = emit_ir(source);
    assert!(eir.contains("runtime.array_reduce_sum_float_seed"), "{eir}");
    assert!(!eir.contains("runtime.array_reduce "), "{eir}");
}

/// An initial value whose raw type is not proven before lowering keeps the generic reduce and
/// is evaluated exactly once, including side effects.
#[test]
fn test_reduce_sum_seed_keeps_generic_for_unproven_initial() {
    let source = r#"<?php
$values = [1, 2, 3];
$stringSeed = "10";
echo array_reduce($values, fn($c, $v) => $c + $v, $stringSeed), "|";
$boolSeed = true;
echo array_reduce($values, fn($c, $v) => $c + $v, $boolSeed), "|";
$i = 0;
$exprSeed = 10 + $i++;
echo array_reduce($values, fn($c, $v) => $c + $v, $exprSeed), "|";
echo $i, "|";
$other = 5;
$refSeed =& $other;
echo array_reduce($values, fn($c, $v) => $c + $v, $refSeed), "|";
$refSeed = 6;
echo array_reduce($values, fn($c, $v) => $c + $v, $refSeed), "\n";
"#;
    assert_eq!(compile_and_run(source), "16|7|16|1|11|12\n");
    let main = emit_main_ir(source);
    assert_eq!(main.matches("runtime.array_reduce ").count(), 5, "{main}");
    assert!(!main.contains("runtime.array_reduce_sum_seed"), "{main}");
    assert!(!main.contains("runtime.array_reduce_sum_float_seed"), "{main}");
}

/// Both raw reducers release their final boxed result without leaks.
#[test]
fn test_reduce_sum_seed_float_and_int_heap_debug_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = range(1, 5);
$seed = 10;
$intSum = array_reduce($values, fn($c, $v) => $c + $v, $seed);
$floatSeed = 1.5;
$floatSum = array_reduce($values, fn($c, $v) => $c + $v, $floatSeed);
$empty = array_slice($values, 0, 0);
$emptySum = array_reduce($empty, fn($c, $v) => $c + $v, 2.5);
echo $intSum, "|", $floatSum, "|", $emptySum, "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "25|16.5|2.5\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Both seeded reducers produce the same output with the EIR optimizer on and off.
#[test]
fn test_reduce_sum_seed_float_and_int_ir_opt_parity() {
    let source = r#"<?php
$values = range(1, 100);
$sum = 0;
$seed = 10;
$floatSeed = 1.5;
for ($i = 0; $i < 3; $i++) {
    $sum += array_reduce($values, fn($c, $v) => $c + $v, $seed);
    $sum += array_reduce($values, fn($c, $v) => $c + $v, $floatSeed);
}
echo $sum, "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "30334.5\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "30334.5\n");
}

/// Every supported target emits the raw float-seeded reducer call.
#[test]
fn test_reduce_sum_seed_float_emits_all_supported_targets() {
    let source = r#"<?php
#[Export]
function reduceFloatSeed(float $seed): float {
    $values = range(1, 10);
    return (float)array_reduce($values, fn($c, $v) => $c + $v, $seed);
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
        assert!(assembly.contains("__rt_array_sum_float_i64"), "{target}");
        assert!(!assembly.contains("__rt_array_reduce_boxed"), "{target}");
        if target == "linux-x86_64" {
            assert!(assembly.contains("mov rdx, QWORD PTR [rdi]"), "{target}");
        } else {
            assert!(assembly.contains("ldr x2, [x0]"), "{target}");
        }
    }
}
