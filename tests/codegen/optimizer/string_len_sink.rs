//! Purpose:
//! Performance-regression coverage for the length-only string transforms.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - `strlen()` of the case transforms, `ucwords()`, and the three-argument `strtr()`
//!   must fold to a raw `str_len` without keeping the pinned subject or the producer call.
//! - Argument side effects still run once, and `--ir-opt=off` keeps the full transform.

use crate::support::*;

/// Emits the `main` function body for one source fixture.
fn emit_main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_string_len_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write string-len IR fixture");
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

/// The length-only producers leave a raw `str_len` and no pinned argument temporary.
#[test]
fn length_only_transforms_use_raw_str_len() {
    for (source, producer) in [
        (
            r#"<?php
$s = str_repeat('abc', 40);
echo strlen(strtr($s, 'abc', 'xyz'));
"#,
            "runtime.strtr",
        ),
        (
            r#"<?php
$s = 'the quick brown fox';
echo strlen(ucwords($s));
"#,
            "runtime.ucwords",
        ),
        (
            r#"<?php
$s = 'The Quick Brown Fox';
echo strlen(strtolower($s));
"#,
            "runtime.strtolower",
        ),
    ] {
        let ir = emit_main_ir(source);
        assert!(ir.contains("str_len"), "missing str_len: {ir}");
        assert!(!ir.contains(producer), "producer survived: {producer}\n{ir}");
        assert!(!ir.contains("push_call_operand_owner"), "{ir}");
    }
}

/// Folding the length keeps `strtr()` argument evaluation and side effects.
#[test]
fn length_only_strtr_keeps_argument_side_effects() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$calls = 0;
function replacement() {
    global $calls;
    $calls++;
    return "abc";
}
$s = str_repeat("abc", 10);
echo strlen(strtr($s, replacement(), "xyz")), '|', $calls, "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "30|1\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Disabling EIR optimization keeps the full transform and the same length.
#[test]
fn length_only_strtr_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
$s = str_repeat('abc', 10);
echo strlen(strtr($s, 'abc', 'xyz')), '|', strlen(ucwords('the quick fox')), "\n";
"#,
        );
        assert_eq!(out, "30|13\n");
    });
}

/// Length-only `chunk_split()` observations match PHP for every chunk shape.
#[test]
fn chunk_split_length_sink_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
echo strlen(chunk_split('abcdef', 2, '-')), '|';
echo strlen(chunk_split('', 3, '-')), '|';
echo strlen(chunk_split('abc', 76, "\r\n")), '|';
echo strlen(chunk_split('abc', 2, '')), '|';
echo strlen(chunk_split('abc')), '|';
try { echo strlen(chunk_split('abc', 0)); } catch (ValueError $e) { echo 'VE'; }
echo "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "9|1|5|3|5|VE\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// The length-only split uses the dedicated helper and drops the allocating call.
#[test]
fn chunk_split_length_sink_ir_uses_helper() {
    let ir = emit_main_ir(
        r#"<?php
$s = str_repeat('abcdef', 20);
echo strlen(chunk_split($s, 76, "\r\n"));
"#,
    );
    assert!(ir.contains("runtime.chunk_split_len"), "{ir}");
    assert!(!ir.contains("runtime.chunk_split "), "{ir}");
}

/// Disabling EIR optimization keeps the allocating split and the same lengths.
#[test]
fn chunk_split_length_sink_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
echo strlen(chunk_split('abcdef', 2, '-')), '|', strlen(chunk_split('abc')), "\n";
"#,
        );
        assert_eq!(out, "9|5\n");
    });
}

/// Length-only `bin2hex()` observations match PHP for binary and empty input.
#[test]
fn bin2hex_length_sink_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
echo strlen(bin2hex('abc')), '|';
echo strlen(bin2hex("\x00\xff")), '|';
echo strlen(bin2hex('')), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "6|4|0\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// The length-only encoder uses the dedicated helper and drops the allocating call.
#[test]
fn bin2hex_length_sink_ir_uses_helper() {
    let ir = emit_main_ir(
        r#"<?php
$s = str_repeat('abc', 40);
echo strlen(bin2hex($s));
"#,
    );
    assert!(ir.contains("runtime.bin2hex_len"), "{ir}");
    assert!(!ir.contains("runtime.bin2hex "), "{ir}");
}

/// Disabling EIR optimization keeps the allocating encoder and the same lengths.
#[test]
fn bin2hex_length_sink_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
echo strlen(bin2hex('abc')), '|', strlen(bin2hex("\x00\xff")), "\n";
"#,
        );
        assert_eq!(out, "6|4\n");
    });
}
