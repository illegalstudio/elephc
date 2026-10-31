//! Purpose:
//! End-to-end coverage for the allocation-free `strlen(base64_decode(...))` sink.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Lax/strict, padded/unpadded, whitespace and malformed inputs compare against PHP.
//! - The IR guard asserts the length helper and no boxed decode; `--ir-opt=off` keeps the
//!   real decoder with the same lengths.

use crate::support::*;

/// Emits the `main` function body for one source fixture.
fn emit_main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_base64_len_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write base64 IR fixture");
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

/// Length-only base64 observations match PHP in lax and strict modes.
#[test]
fn base64_decode_len_sink_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
echo strlen(base64_decode('SGVsbG8=')), '|';
echo strlen(base64_decode('SGVsbG8')), '|';
echo strlen(base64_decode('SGVs bG8=')), '|';
echo strlen(base64_decode('A===')), '|';
echo strlen(base64_decode('A===', true)), '|';
echo strlen(base64_decode('SGVsbG8==', true)), '|';
echo strlen(base64_decode('SGVsbG8=', true)), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "5|5|5|0|0|0|5\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// The sink uses the dedicated length helper and drops the allocating decoder.
#[test]
fn base64_decode_len_sink_ir_uses_helper() {
    let ir = emit_main_ir(
        r#"<?php
$s = base64_encode(str_repeat('abc', 40));
echo strlen(base64_decode($s));
"#,
    );
    assert!(ir.contains("runtime.base64_decoded_len"), "{ir}");
    assert!(!ir.contains("runtime.base64_decode "), "{ir}");
}

/// Disabling EIR optimization keeps the real decoder and the same lengths.
#[test]
fn base64_decode_len_sink_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
echo strlen(base64_decode('SGVsbG8=')), '|', strlen(base64_decode('A===', true)), "\n";
"#,
        );
        assert_eq!(out, "5|0\n");
    });
}
