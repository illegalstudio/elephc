//! Purpose:
//! End-to-end coverage for folding `strlen`, constant string offsets and `ord()` over
//! compile-time constant strings.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Folded observations must reproduce PHP byte semantics, including negative offsets,
//!   `ord("") === 0`, and the byte-marker convention for non-ASCII bytes.
//! - An out-of-range offset must keep the dynamic warning path, and `--ir-opt=on` and
//!   `--ir-opt=off` must agree on every fixture.

use crate::support::*;

/// Emits the optimized EIR text for one fixture.
fn optimized_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_const_string_observations_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write const string observation fixture");
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

/// Compiles and runs one fixture with an explicit optimizer mode.
fn run_with_ir_opt(source: &str, mode: &str) -> String {
    let dir = make_cli_test_dir("elephc_const_string_observations_run");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write const string observation fixture");
    let compiled = elephc_cli_command(&dir)
        .arg(mode)
        .arg(&path)
        .output()
        .expect("compile const string observation fixture");
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

/// A literal local's length, constant offset and `ord()` become constants.
#[test]
fn test_const_string_observations_fold_literal_local() {
    let source = r#"<?php
$text = "abc";
$n = 0;
for ($i = 0; $i < 1000 + $argc; $i++) {
    $n += strlen($text) + ord($text[1]) + ord($text[-1]);
}
echo $n, "\n";
"#;
    let expected = "200200\n";
    assert_eq!(compile_and_run(source), expected);
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), expected);
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), expected);
    let ir = optimized_ir(source);
    assert!(!ir.contains("str_len"), "{ir}");
    assert!(!ir.contains("str_char_at"), "{ir}");
    assert!(!ir.contains("runtime.ord"), "{ir}");
}

/// `ord("")` folds to zero, while an out-of-range offset keeps PHP's warning.
#[test]
fn test_const_string_observations_keep_oob_warning() {
    let source = r#"<?php
$empty = "";
$text = "abc";
echo ord($empty), "|";
echo ord($text[9]), "\n";
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "0|0\n");
    assert!(out.stderr.contains("Uninitialized string offset 9"), "{}", out.stderr);
    let ir = optimized_ir(source);
    assert!(ir.contains("str_char_at"), "{ir}");
}

/// Constant mbstring producers feed the same observation fold inside a hot loop.
#[test]
fn test_const_string_observations_fold_mbstring_producers() {
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
    let expected = run_with_ir_opt(source, "--ir-opt=off");
    assert_eq!(compile_and_run(source), expected);
    let ir = optimized_ir(source);
    assert!(!ir.contains("str_len"), "{ir}");
    assert!(!ir.contains("str_char_at"), "{ir}");
    assert!(!ir.contains("runtime.ord"), "{ir}");
}
