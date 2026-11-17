//! Purpose:
//! End-to-end coverage for compile-time folding of constant mbstring producers.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Only canonical full-decimal numeric-entity maps, literal UTF-8 encodings and
//!   literal subjects fold; dynamic subjects, modes, encodings and other maps must
//!   keep the runtime bridge call and PHP's exact result.
//! - Assertions cover EIR call removal, output parity, heap cleanliness, IR-opt and
//!   no-IR-opt behavior, and assembly emission on every supported target.

use crate::support::*;

/// Emits optimized or unoptimized EIR text for one fixture.
fn emit_ir(source: &str, ir_opt: bool) -> String {
    let dir = make_cli_test_dir("elephc_mb_constant_fold_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write mb constant fold fixture");
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

/// Compiles and runs one fixture with an explicit EIR optimizer mode.
fn run_with_ir_opt(source: &str, mode: &str) -> String {
    let dir = make_cli_test_dir("elephc_mb_constant_fold_run");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write mb constant fold run fixture");
    let compiled = elephc_cli_command(&dir)
        .arg(mode)
        .arg(&path)
        .output()
        .expect("compile mb constant fold fixture");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = std::process::Command::new(dir.join("main"))
        .output()
        .expect("run mb constant fold fixture");
    let _ = fs::remove_dir_all(dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("mb constant fold output is UTF-8")
}

/// Emits target assembly for one fixture, using staticlib output for every target.
fn emit_target_assembly(source: &str, target: &str) -> String {
    let dir = make_cli_test_dir("elephc_mb_constant_fold_targets");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write mb constant fold target fixture");
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
        .expect("read mb constant fold target assembly");
    let _ = fs::remove_dir_all(dir);
    assembly
}

/// A literal `mb_convert_kana()` subject, mode and encoding fold to one constant.
#[test]
fn test_mb_convert_kana_constant_folds() {
    let source = r#"<?php
$input = "ＡＢＣ１２３ａｂｃ";
$n = 0;
for ($i = 0; $i < 3; $i++) {
    $converted = mb_convert_kana($input, "a", "UTF-8");
    $n += strlen($converted) + ord($converted[0]);
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "222\n");
    let optimized = emit_ir(source, true);
    assert!(!optimized.contains("runtime.mb_convert_kana"), "{optimized}");
    let unoptimized = emit_ir(source, false);
    assert!(unoptimized.contains("runtime.mb_convert_kana"), "{unoptimized}");
}

/// Canonical-map numeric-entity encode/decode calls over inlined literal sources fold.
#[test]
fn test_mb_numeric_entity_constant_folds() {
    let source = r#"<?php
function ne_source(): string { return "café 猫 😀 <&>"; }
function nd_source(): string { return "caf&#233; &#29483; &#128512; <&>"; }
$map = [128, 1114111, 0, 2097151];
$n = 0;
for ($i = 0; $i < 3; $i++) {
    $e = mb_encode_numericentity(ne_source(), $map, "UTF-8");
    $d = mb_decode_numericentity(nd_source(), $map, "UTF-8");
    $n += strlen($e) + ord($e[0]) + strlen($d) + ord($d[0]);
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "744\n");
    let optimized = emit_ir(source, true);
    assert!(!optimized.contains("runtime.mb_encode_numericentity"), "{optimized}");
    assert!(!optimized.contains("runtime.mb_decode_numericentity"), "{optimized}");
    let unoptimized = emit_ir(source, false);
    assert!(unoptimized.contains("runtime.mb_encode_numericentity"), "{unoptimized}");
    assert!(unoptimized.contains("runtime.mb_decode_numericentity"), "{unoptimized}");
}

/// A dynamic subject keeps the runtime call and PHP's exact result.
#[test]
fn test_mb_constant_fold_dynamic_subject_falls_back() {
    let source = r#"<?php
$source = $argc > 0 ? "café 猫" : "x";
$map = [128, 1114111, 0, 2097151];
$encoded = mb_encode_numericentity($source, $map, "UTF-8");
echo strlen($encoded), "\n";
"#;
    assert_eq!(compile_and_run(source), "18\n");
    let optimized = emit_ir(source, true);
    assert!(optimized.contains("runtime.mb_encode_numericentity"), "{optimized}");
}

/// A non-canonical numeric-entity map keeps the runtime call and its exact map.
#[test]
fn test_mb_constant_fold_noncanonical_map_falls_back() {
    let source = r#"<?php
$map = [65, 90, 0, 0];
$encoded = mb_encode_numericentity("AZ", $map, "UTF-8");
echo $encoded, "\n";
"#;
    assert_eq!(compile_and_run(source), "&#0;&#0;\n");
    let optimized = emit_ir(source, true);
    assert!(optimized.contains("runtime.mb_encode_numericentity"), "{optimized}");
}

/// A dynamic kana mode or encoding keeps the runtime call and its exact result.
#[test]
fn test_mb_constant_fold_dynamic_mode_and_encoding_fall_back() {
    let source = r#"<?php
$input = "ＡＢＣ";
$mode = $argc > 0 ? "a" : "r";
$converted = mb_convert_kana($input, $mode, "UTF-8");
echo $converted, "|";
$encoding = $argc > 0 ? "UTF-8" : "ISO-8859-1";
$map = [128, 1114111, 0, 2097151];
$encoded = mb_encode_numericentity("café", $map, $encoding);
echo strlen($encoded), ":", ord($encoded[0]), "\n";
"#;
    assert_eq!(compile_and_run(source), "ABC|9:99\n");
    let optimized = emit_ir(source, true);
    assert!(optimized.contains("runtime.mb_convert_kana"), "{optimized}");
    assert!(optimized.contains("runtime.mb_encode_numericentity"), "{optimized}");
}

/// An empty literal subject folds to an empty persistent string.
#[test]
fn test_mb_constant_fold_empty_input() {
    let source = r#"<?php
$empty = mb_convert_kana("", "a", "UTF-8");
echo strlen($empty), "\n";
"#;
    assert_eq!(compile_and_run(source), "0\n");
    let optimized = emit_ir(source, true);
    assert!(!optimized.contains("runtime.mb_convert_kana"), "{optimized}");
}

/// The folded and runtime paths agree with the EIR optimizer on and off.
#[test]
fn test_mb_constant_fold_ir_opt_parity() {
    let source = r#"<?php
$input = "ＡＢＣ１２３ａｂｃ";
$n = 0;
for ($i = 0; $i < 3; $i++) {
    $converted = mb_convert_kana($input, "a", "UTF-8");
    $n += strlen($converted) + ord($converted[0]);
}
echo $n, "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "222\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "222\n");
}

/// Folded persistent results and the surrounding string locals stay heap-clean.
#[test]
fn test_mb_constant_fold_heap_debug_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$input = "ＡＢＣ１２３ａｂｃ";
$map = [128, 1114111, 0, 2097151];
$n = 0;
for ($i = 0; $i < 3; $i++) {
    $converted = mb_convert_kana($input, "a", "UTF-8");
    $encoded = mb_encode_numericentity("café 猫", $map, "UTF-8");
    $n += strlen($converted) + strlen($encoded) + ord($encoded[0]);
}
echo $n, "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "378\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Every supported target emits the folded source without the runtime mb call.
#[test]
fn test_mb_constant_fold_emits_all_supported_targets() {
    let source = r#"<?php
$converted = mb_convert_kana("ＡＢＣ", "a", "UTF-8");
echo strlen($converted), "\n";
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let assembly = emit_target_assembly(source, target);
        assert!(!assembly.is_empty(), "{target}: empty assembly");
        assert!(
            !assembly.contains("mb_convert_kana"),
            "{target}: runtime mb call remains\n{assembly}"
        );
    }
}

/// A valid constant non-UTF-8 encoding may need the global substitute character,
/// so it keeps the runtime bridge call.
#[test]
fn test_mb_constant_fold_non_utf8_encoding_falls_back() {
    let source = r#"<?php
$converted = mb_convert_kana("ABC", "a", "ISO-8859-1");
echo $converted, "|";
$map = [128, 1114111, 0, 2097151];
$encoded = mb_encode_numericentity("café", $map, "ISO-8859-1");
echo strlen($encoded), ":", ord($encoded[0]), "\n";
"#;
    assert_eq!(compile_and_run(source), "ABC|15:99\n");
    let optimized = emit_ir(source, true);
    assert!(optimized.contains("runtime.mb_convert_kana"), "{optimized}");
    assert!(optimized.contains("runtime.mb_encode_numericentity"), "{optimized}");
}

/// A marker-encoded high byte is invalid UTF-8 as a PHP subject, so the runtime bridge owns
/// its substitution behavior and the fold stays disabled.
#[test]
fn test_mb_constant_fold_rejects_escaped_byte_subject() {
    let source = r#"<?php
$map = [128, 1114111, 0, 2097151];
$text = "caf\xFF";
$encoded = mb_encode_numericentity($text, $map, "UTF-8");
echo strlen($encoded), ":", ord($encoded[0]), "|";
$converted = mb_convert_kana("caf\xFF", "a", "UTF-8");
echo strlen($converted), ":", ord($converted[0]), "\n";
"#;
    let optimized = run_with_ir_opt(source, "--ir-opt=on");
    let unoptimized = run_with_ir_opt(source, "--ir-opt=off");
    assert_eq!(optimized, unoptimized);
    let ir = emit_ir(source, true);
    assert!(ir.contains("runtime.mb_encode_numericentity"), "{ir}");
    assert!(ir.contains("runtime.mb_convert_kana"), "{ir}");
}

/// Surrogate entities keep the runtime decoder, which owns their raw three-byte encoding.
#[test]
fn test_mb_constant_fold_rejects_surrogate_decode() {
    let source = r#"<?php
$map = [128, 1114111, 0, 2097151];
$decoded = mb_decode_numericentity("&#xD800;", $map, "UTF-8");
echo strlen($decoded), ":", ord($decoded[0]), "\n";
"#;
    let optimized = run_with_ir_opt(source, "--ir-opt=on");
    let unoptimized = run_with_ir_opt(source, "--ir-opt=off");
    assert_eq!(optimized, unoptimized);
    let ir = emit_ir(source, true);
    assert!(ir.contains("runtime.mb_decode_numericentity"), "{ir}");
}

/// A folded character in the pool's marker range is re-escaped so codegen emits its UTF-8
/// bytes instead of reading U+E000 as a NUL byte.
#[test]
fn test_mb_constant_fold_preserves_marker_range_output() {
    let source = r#"<?php
$map = [128, 1114111, 0, 2097151];
$text = mb_decode_numericentity("&#57344;&#57345;", $map, "UTF-8");
echo strlen($text), ":", bin2hex($text), "\n";
"#;
    let expected = "6:ee8080ee8081\n";
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), expected);
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), expected);
    let optimized = emit_ir(source, true);
    assert!(!optimized.contains("runtime.mb_decode_numericentity"), "{optimized}");
}
