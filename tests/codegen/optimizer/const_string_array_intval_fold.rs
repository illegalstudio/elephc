//! Purpose:
//! End-to-end coverage for folding `(int) $texts[$i & M]` over an immutable literal
//! string array into a packed `array<int>` and a direct unchecked element load.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - The fold must reproduce PHP's leading-numeric-run grammar exactly: signed and
//!   whitespace-prefixed integers, `42abc`, `0x1A`, `1_000`, `007`, float forms, exponent
//!   forms and out-of-range saturation.
//! - A dynamic index, mismatched mask, observed or mutated array, non-entry construction,
//!   runtime-built source and explicit-base `intval()` must keep the generic checked read
//!   and the runtime string conversion.
//! - Assertions cover EIR target selection, `--ir-opt` parity, heap cleanliness and
//!   per-target compilation.

use crate::support::*;

/// Emits the optimized or unoptimized EIR module text of one fixture.
fn emit_ir(source: &str, ir_opt: bool) -> String {
    let dir = make_cli_test_dir("elephc_const_string_array_intval_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write const string array fixture");
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
    let dir = make_cli_test_dir("elephc_const_string_array_intval_run");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write const string array run fixture");
    let compiled = elephc_cli_command(&dir)
        .arg(mode)
        .arg(&path)
        .output()
        .expect("compile const string array fixture");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = std::process::Command::new(dir.join("main"))
        .output()
        .expect("run const string array fixture");
    let _ = fs::remove_dir_all(dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("const string array output is UTF-8")
}

/// The benchmark shape folds into one unchecked packed-int read per iteration.
#[test]
fn test_const_string_array_intval_fold_benchmark_shape() {
    let source = r#"<?php
$texts = [
    "123456789012345678", "  -12345678901234567", "42abc", "0x1A",
    "1_000", "007", "  -99  ", "999999999999999999",
];
$n = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    $n = (($n << 5) ^ intval($texts[$i & 7])) & 0xFFFFFFFF;
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "1285328991\n");
    let main = main_ir(source);
    assert!(main.contains("php=array<int>"), "{main}");
    assert!(main.contains("array_get_unchecked"), "{main}");
    assert!(!main.contains("array_get v"), "{main}");
}

/// An explicit `(int)` cast takes the same fold as `intval()`.
#[test]
fn test_const_string_array_intval_fold_explicit_cast() {
    let source = r#"<?php
$texts = ["123456789012345678", "  -12345678901234567", "42abc", "0x1A", "1_000", "007", "  -99  ", "999999999999999999"];
$n = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    $n = (($n << 5) ^ (int)$texts[$i & 7]) & 0xFFFFFFFF;
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "1285328991\n");
    let main = main_ir(source);
    assert!(main.contains("array_get_unchecked"), "{main}");
}

/// Signed, blank-padded, float, exponent, hex-like and underscore strings match PHP.
#[test]
fn test_const_string_array_intval_fold_edge_strings() {
    let source = r#"<?php
$texts = [
    "999999999999999999999", "-999999999999999999999", "1e3", "1.5",
    ".5", "  +42  ", "0x1A", "1_000",
];
$n = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    $n = (($n << 5) ^ intval($texts[$i & 7])) & 0xFFFFFFFF;
}
echo $n, "\n";
$other = ["abc", "-", "  - 1", "+7", "00042", "-0", "1e", "5."];
$m = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    $m = (($m << 5) ^ intval($other[$i & 7])) & 0xFFFFFFFF;
}
echo $m, "\n";
"#;
    assert_eq!(compile_and_run(source), "3490752513\n6619173\n");
    let main = main_ir(source);
    assert_eq!(main.matches("array_get_unchecked").count(), 2, "{main}");
}

/// Empty, blank-only and sign-only strings fold to the same zero PHP produces.
#[test]
fn test_const_string_array_intval_fold_empty_and_blank_strings() {
    let source = r#"<?php
$texts = ["", " ", "\t", "  - 1", "+", "-", "1e", "5."];
$n = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    $n = (($n << 5) ^ intval($texts[$i & 7])) & 0xFFFFFFFF;
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "37\n");
    let main = main_ir(source);
    assert_eq!(main.matches("array_get_unchecked").count(), 1, "{main}");
}

/// A literal array built in a user function's entry block folds there too.
#[test]
fn test_const_string_array_intval_fold_inside_function() {
    let source = r#"<?php
function checksum(): int {
    $texts = ["123456789012345678", "  -12345678901234567", "42abc", "0x1A", "1_000", "007", "  -99  ", "999999999999999999"];
    $n = 0;
    for ($i = 0; $i < 3000; $i++) {
        $n = (($n << 5) ^ intval($texts[$i & 7])) & 0xFFFFFFFF;
    }
    return $n;
}
echo checksum(), "\n";
"#;
    assert_eq!(compile_and_run(source), "1285328991\n");
    let ir = emit_ir(source, true);
    assert_eq!(ir.matches("array_get_unchecked").count(), 1, "{ir}");
}

/// A folded cast used directly as a branch condition still folds.
#[test]
fn test_const_string_array_intval_fold_condition_use() {
    let source = r#"<?php
$texts = ["1", "2", "3", "4", "5", "6", "7", "999999999999999999"];
$count = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    if ((int)$texts[$i & 7] > 50) {
        $count++;
    }
}
echo $count, "\n";
"#;
    assert_eq!(compile_and_run(source), "375\n");
    let main = main_ir(source);
    assert_eq!(main.matches("array_get_unchecked").count(), 1, "{main}");
}

/// A one-element literal uses mask zero and still folds.
#[test]
fn test_const_string_array_intval_fold_single_element() {
    let source = r#"<?php
$texts = ["  +42abc"];
$n = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    $n = (($n << 5) ^ intval($texts[$i & 0])) & 0xFFFFFFFF;
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "3602230634\n");
    let main = main_ir(source);
    assert_eq!(main.matches("array_get_unchecked").count(), 1, "{main}");
}

/// A dynamic `% 8` index keeps the checked read and runtime conversion.
#[test]
fn test_const_string_array_intval_fold_dynamic_index_falls_back() {
    let source = r#"<?php
$texts = ["1", "2", "3", "4", "5", "6", "7", "999999999999999999"];
$n = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    $n = (($n << 5) ^ intval($texts[$i % 8])) & 0xFFFFFFFF;
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "555837215\n");
    let main = main_ir(source);
    assert!(!main.contains("array_get_unchecked"), "{main}");
    assert!(main.contains("array_get v"), "{main}");
}

/// A mask shorter than the literal keeps the checked read.
#[test]
fn test_const_string_array_intval_fold_mask_length_mismatch_falls_back() {
    let source = r#"<?php
$texts = ["1", "2", "3", "4", "5", "6", "7", "999999999999999999"];
$n = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    $n = (($n << 5) ^ intval($texts[$i & 3])) & 0xFFFFFFFF;
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "2252376164\n");
    let main = main_ir(source);
    assert!(!main.contains("array_get_unchecked"), "{main}");
}

/// A non-power-of-two mask keeps the checked read even when every index is in bounds.
#[test]
fn test_const_string_array_intval_fold_non_power_of_two_mask_falls_back() {
    let source = r#"<?php
$texts = ["1", "2", "3", "4", "5", "6", "7", "999999999999999999"];
$n = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    $n = (($n << 5) ^ intval($texts[$i & 6])) & 0xFFFFFFFF;
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "1177720039\n");
    let main = main_ir(source);
    assert!(!main.contains("array_get_unchecked"), "{main}");
}

/// Observing the literal array after the loop keeps the checked read.
#[test]
fn test_const_string_array_intval_fold_observed_array_falls_back() {
    let source = r#"<?php
$texts = ["1", "2", "3", "4", "5", "6", "7", "999999999999999999"];
$n = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    $n = (($n << 5) ^ intval($texts[$i & 7])) & 0xFFFFFFFF;
}
echo $n, "|", $texts[0], "\n";
"#;
    assert_eq!(compile_and_run(source), "555837215|1\n");
    let main = main_ir(source);
    assert!(!main.contains("array_get_unchecked"), "{main}");
}

/// Mutating an element inside the loop keeps the checked read.
#[test]
fn test_const_string_array_intval_fold_mutation_falls_back() {
    let source = r#"<?php
$texts = ["1", "2", "3", "4", "5", "6", "7", "999999999999999999"];
$n = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    $n = (($n << 5) ^ intval($texts[$i & 7])) & 0xFFFFFFFF;
    $texts[0] = (string)$n;
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "555837215\n");
    let main = main_ir(source);
    assert!(!main.contains("array_get_unchecked"), "{main}");
}

/// A literal constructed in a branch instead of the entry block keeps the checked read.
#[test]
fn test_const_string_array_intval_fold_non_entry_construction_falls_back() {
    let source = r#"<?php
if ($argc > 0) {
    $texts = ["1", "2", "3", "4", "5", "6", "7", "999999999999999999"];
} else {
    $texts = ["8", "9", "10", "11", "12", "13", "14", "15"];
}
$n = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    $n = (($n << 5) ^ intval($texts[$i & 7])) & 0xFFFFFFFF;
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "555837215\n");
    let main = main_ir(source);
    assert!(!main.contains("array_get_unchecked"), "{main}");
}

/// A runtime-built string array keeps the checked read.
#[test]
fn test_const_string_array_intval_fold_runtime_built_array_falls_back() {
    let source = r#"<?php
$texts = [];
for ($i = 0; $i < 8; $i++) {
    $texts[] = (string)($i + 2);
}
$n = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    $n = (($n << 5) ^ intval($texts[$i & 7])) & 0xFFFFFFFF;
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "3360890121\n");
    let main = main_ir(source);
    assert!(!main.contains("array_get_unchecked"), "{main}");
}

/// An explicit-base `intval()` lowers to a runtime call, never this fold.
#[test]
fn test_const_string_array_intval_fold_explicit_base_falls_back() {
    let source = r#"<?php
$texts = ["10", "11", "12", "13", "14", "15", "16", "17"];
$n = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    $n = (($n << 5) ^ intval($texts[$i & 7], 16)) & 0xFFFFFFFF;
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "1698322135\n");
    let main = main_ir(source);
    assert!(!main.contains("array_get_unchecked"), "{main}");
}

/// The folded and generic paths agree with the EIR optimizer on and off.
#[test]
fn test_const_string_array_intval_fold_ir_opt_parity() {
    let source = r#"<?php
$texts = ["123456789012345678", "  -12345678901234567", "42abc", "0x1A", "1_000", "007", "  -99  ", "999999999999999999"];
$n = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    $n = (($n << 5) ^ intval($texts[$i & 7])) & 0xFFFFFFFF;
}
echo $n, "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "1285328991\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "1285328991\n");
    assert!(
        emit_ir(source, true).contains("array_get_unchecked"),
        "optimized EIR must fold the literal read"
    );
    assert!(
        !emit_ir(source, false).contains("array_get_unchecked"),
        "unoptimized EIR must keep the checked read"
    );
}

/// The packed integer replacement array and its element reads stay heap-clean.
#[test]
fn test_const_string_array_intval_fold_heap_debug_is_clean() {
    let source = r#"<?php
$texts = ["123456789012345678", "  -12345678901234567", "42abc", "0x1A", "1_000", "007", "  -99  ", "999999999999999999"];
$n = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    $n = (($n << 5) ^ intval($texts[$i & 7])) & 0xFFFFFFFF;
}
echo $n, "\n";
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "1285328991\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Every supported target compiles the folded literal array.
#[test]
fn test_const_string_array_intval_fold_emits_all_supported_targets() {
    let source = r#"<?php
$texts = ["123456789012345678", "  -12345678901234567", "42abc", "0x1A", "1_000", "007", "  -99  ", "999999999999999999"];
$n = 0;
for ($i = 0; $i < 2999 + $argc; $i++) {
    $n = (($n << 5) ^ intval($texts[$i & 7])) & 0xFFFFFFFF;
}
echo $n, "\n";
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let dir = make_cli_test_dir("elephc_const_string_array_intval_targets");
        let path = dir.join("main.php");
        fs::write(&path, source).expect("write const string array target fixture");
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
            .expect("read const string array target assembly");
        assert!(!assembly.is_empty(), "{target}: empty assembly");
        let _ = fs::remove_dir_all(dir);
    }
}
