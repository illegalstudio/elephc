//! Purpose:
//! End-to-end coverage for the allocation-free `strlen(strstr(...))` sink.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Hit, miss, `$before_needle`, empty needle and runtime-local haystack cases compare
//!   against PHP; heap debug verifies the retired lifetime pin does not leak.
//! - A boxed `string|false` value observed as a string keeps the original `strstr()` call.

use crate::support::*;

/// Length-only `strstr()` observations match PHP under every result shape.
#[test]
fn strstr_len_sink_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$s = str_repeat('a', 40) . 'Needle' . str_repeat('a', 20) . 'needle';
echo strlen(strstr($s, 'needle')), '|';
echo strlen(strstr($s, 'Needle')), '|';
echo strlen(strstr($s, 'absent')), '|';
echo strlen(strstr($s, 'needle', true)), '|';
echo strlen(strstr($s, '')), '|';
echo strlen(strstr($s, '', true)), '|';
$n = 0;
$iters = $argc + 64;
for ($i = 0; $i < $iters; $i++) { $n += strlen(strstr($s, 'needle')) + $i; }
echo '|', $n, "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "6|32|0|66|72|0||2470\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// A boxed `string|false` value used as a string keeps the original call.
#[test]
fn strstr_boxed_result_survives_value_use() {
    let out = compile_and_run(
        r#"<?php
$s = str_repeat('a', 10) . 'needle';
$hit = strstr($s, 'needle');
$miss = strstr($s, 'absent');
echo $hit === false ? 'bad' : $hit, '|', $miss === false ? 'miss' : 'bad', "\n";
"#,
    );
    assert_eq!(out, "needle|miss\n");
}

/// Disabling EIR optimization keeps the boxed call and the same lengths.
#[test]
fn strstr_len_sink_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
$s = str_repeat('a', 10) . 'needle';
echo strlen(strstr($s, 'needle')), '|', strlen(strstr($s, 'absent')), '|';
echo strlen(strstr($s, 'needle', true)), "\n";
"#,
        );
        assert_eq!(out, "6|0|10\n");
    });
}

/// Emits the `main` function body for one source fixture.
fn emit_main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_strstr_len_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write strstr IR fixture");
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

/// The sink replaces the boxed call and retires the haystack lifetime pin.
#[test]
fn strstr_len_sink_ir_uses_raw_helper() {
    let ir = emit_main_ir(
        r#"<?php
$s = str_repeat('a', 10) . 'needle';
$n = 0;
for ($i = 0; $i < 3; $i++) { $n += strlen(strstr($s, 'needle')) + $i; }
echo $n;
"#,
    );
    assert!(ir.contains("runtime.strstr_len"), "{ir}");
    assert!(!ir.contains("runtime.strstr "), "{ir}");
    assert!(!ir.contains("push_call_operand_owner"), "{ir}");
}
