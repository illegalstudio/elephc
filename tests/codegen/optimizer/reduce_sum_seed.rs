//! Purpose:
//! End-to-end coverage for the seeded raw integer `array_reduce()` sum fusion.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - `array_reduce($ints, fn($c, $v) => $c + $v, C)` with a literal nonzero `C` lowers to
//!   `ArrayReduceSumSeed`, which keeps the carry raw and boxes only the final int-or-float.
//! - Initial zero still uses `array_sum()`, and non-add bodies keep the generic reduce path.
//! - Overflow promotes the carry to float exactly like PHP's left-to-right addition.

use crate::support::*;

/// Emits the optimized `main` function body for one fixture.
fn emit_main_ir(source: &str) -> String {
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
