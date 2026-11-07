//! Purpose:
//! End-to-end coverage for the seeded raw integer `array_reduce()` product fusion.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - `array_reduce($ints, fn($c, $v) => $c * $v, C)` lowers to `ArrayReduceProductSeed`
//!   (raw int seed) or `ArrayReduceProductFloatSeed` (raw float seed).
//! - Integer multiplication uses PHP's checked semantics: the first overflow converts both
//!   operands to double and the rest of the reduction continues in double mode.
//! - Non-multiplication bodies and initials whose raw type is not proven before lowering
//!   keep the generic reduce and are evaluated exactly once.

use crate::support::*;

/// Emits the complete EIR for one fixture.
fn emit_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_reduce_product_seed_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write reduce product fixture");
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

/// Compiles and runs one fixture through the CLI with an explicit EIR optimizer mode.
fn run_with_ir_opt(source: &str, mode: &str) -> String {
    let dir = make_cli_test_dir("elephc_reduce_product_seed_run");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write reduce product run fixture");
    let compiled = elephc_cli_command(&dir)
        .arg(mode)
        .arg(&php_path)
        .output()
        .expect("compile reduce product fixture");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = std::process::Command::new(dir.join("main"))
        .output()
        .expect("run reduce product fixture");
    let _ = fs::remove_dir_all(dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("reduce product output is UTF-8")
}

/// Emits target assembly for one fixture, using staticlib output for every target.
fn emit_target_assembly(source: &str, target: &str) -> String {
    let dir = make_cli_test_dir("elephc_reduce_product_seed_targets");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write reduce product target fixture");
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
        .expect("read reduce product target assembly");
    let _ = fs::remove_dir_all(dir);
    assembly
}

/// Literal and local int seeds, both operand orders, empty ranges and overflow chains use the
/// raw product reducer.
#[test]
fn test_reduce_product_seed_uses_raw_accumulator() {
    let source = r#"<?php
$values = [1, 2, 3, 4];
echo array_reduce($values, fn($c, $v) => $c * $v, 1), "|";
echo array_reduce($values, fn($c, $v) => $v * $c, 2), "|";
$seed = 3;
echo array_reduce($values, fn($c, $v) => $c * $v, $seed), "|";
$empty = array_slice($values, 0, 0);
var_dump(array_reduce($empty, fn($c, $v) => $c * $v, 7));
$big = [PHP_INT_MAX, 2];
var_dump(array_reduce($big, fn($c, $v) => $c * $v, 1));
$min = [PHP_INT_MIN, -1];
var_dump(array_reduce($min, fn($c, $v) => $c * $v, 1));
$neg = [-3, 4];
var_dump(array_reduce($neg, fn($c, $v) => $c * $v, 2));
$zero = [0, PHP_INT_MAX];
var_dump(array_reduce($zero, fn($c, $v) => $c * $v, 5));
$chain = [PHP_INT_MAX, 2, 3];
var_dump(array_reduce($chain, fn($c, $v) => $c * $v, 1));
"#;
    assert_eq!(
        compile_and_run(source),
        "24|48|72|int(7)\n\
         float(1.8446744073709552E+19)\n\
         float(9.223372036854776E+18)\n\
         int(-24)\n\
         int(0)\n\
         float(5.5340232221128655E+19)\n"
    );
    let eir = emit_ir(source);
    assert_eq!(eir.matches("runtime.array_reduce_product_seed").count(), 9, "{eir}");
    assert!(!eir.contains("runtime.array_reduce "), "{eir}");
}

/// Float literal and raw-float local seeds multiply every element in double mode.
#[test]
fn test_reduce_product_seed_float_seeds() {
    let source = r#"<?php
$values = [1, 2, 3];
var_dump(array_reduce($values, fn($c, $v) => $c * $v, 1.5));
$empty = array_slice($values, 0, 0);
var_dump(array_reduce($empty, fn($c, $v) => $c * $v, -0.0));
$inf = array_reduce($values, fn($c, $v) => $c * $v, INF);
var_dump($inf);
$seed = 0.5;
var_dump(array_reduce($values, fn($c, $v) => $c * $v, $seed));
var_dump(array_reduce($values, fn($c, $v) => $c * $v, -0.0));
"#;
    assert_eq!(
        compile_and_run(source),
        "float(9)\nfloat(-0)\nfloat(INF)\nfloat(3)\nfloat(-0)\n"
    );
    let eir = emit_ir(source);
    assert_eq!(
        eir.matches("runtime.array_reduce_product_float_seed").count(),
        5,
        "{eir}"
    );
    assert!(!eir.contains("runtime.array_reduce "), "{eir}");
}

/// Unproven initials and non-multiplication bodies keep the generic reduce and evaluate the
/// initial exactly once, including side effects and reference rebinding.
#[test]
fn test_reduce_product_seed_keeps_generic_for_other_shapes() {
    let source = r#"<?php
$values = [1, 2, 3];
$stringSeed = "2";
echo array_reduce($values, fn($c, $v) => $c * $v, $stringSeed), "|";
$boolSeed = true;
echo array_reduce($values, fn($c, $v) => $c * $v, $boolSeed), "|";
$i = 0;
$exprSeed = 1 + $i++;
echo array_reduce($values, fn($c, $v) => $c * $v, $exprSeed), "|";
echo $i, "|";
$other = 2;
$refSeed =& $other;
echo array_reduce($values, fn($c, $v) => $c * $v, $refSeed), "|";
$refSeed = 3;
echo array_reduce($values, fn($c, $v) => $c * $v, $refSeed), "|";
echo array_reduce($values, fn($c, $v) => $c - $v, 10), "\n";
"#;
    assert_eq!(compile_and_run(source), "12|6|6|1|12|18|4\n");
    let eir = emit_ir(source);
    assert_eq!(eir.matches("runtime.array_reduce ").count(), 6, "{eir}");
    assert!(!eir.contains("runtime.array_reduce_product_seed"), "{eir}");
    assert!(!eir.contains("runtime.array_reduce_product_float_seed"), "{eir}");
}

/// Both raw product reducers release their final boxed result without leaks.
#[test]
fn test_reduce_product_seed_heap_debug_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = range(1, 5);
$seed = 2;
$intProduct = array_reduce($values, fn($c, $v) => $c * $v, $seed);
$floatSeed = 0.5;
$floatProduct = array_reduce($values, fn($c, $v) => $c * $v, $floatSeed);
$big = [PHP_INT_MAX, 2];
$overflow = array_reduce($big, fn($c, $v) => $c * $v, 1);
echo $intProduct, "|", $floatProduct, "|", gettype($overflow), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "240|60|double\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Both product reducers produce the same output with the EIR optimizer on and off.
#[test]
fn test_reduce_product_seed_ir_opt_parity() {
    let source = r#"<?php
$values = range(1, 10);
$sum = 0;
$intSeed = 2;
$floatSeed = 0.5;
for ($i = 0; $i < 3; $i++) {
    $sum += array_reduce($values, fn($c, $v) => $c * $v, $intSeed);
    $sum += array_reduce($values, fn($c, $v) => $c * $v, $floatSeed);
}
echo $sum, "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "27216000\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "27216000\n");
}

/// Every supported target emits both raw product entries and no generic boxed reduce.
#[test]
fn test_reduce_product_seed_emits_all_supported_targets() {
    let source = r#"<?php
#[Export]
function productInt(int $seed): int {
    $values = range(1, 10);
    return (int)array_reduce($values, fn($c, $v) => $c * $v, $seed);
}
#[Export]
function productFloat(float $seed): float {
    $values = range(1, 10);
    return (float)array_reduce($values, fn($c, $v) => $c * $v, $seed);
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
        assert!(assembly.contains("__rt_array_product_int_i64"), "{target}");
        assert!(assembly.contains("__rt_array_product_float_i64"), "{target}");
        assert!(!assembly.contains("__rt_array_reduce_boxed"), "{target}");
        // The helper body lives in the shared runtime object; this assembly only proves the
        // target-aware lowering materializes the array and seed operands for the call.
        if target == "linux-x86_64" {
            assert!(assembly.contains("mov rdx, QWORD PTR [rdi]"), "{target}");
        } else {
            assert!(assembly.contains("ldr x2, [x0]"), "{target}");
        }
    }
}
