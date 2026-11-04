//! Purpose:
//! End-to-end coverage for the allocation-free `strlen(inet_pton(...))` sink.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - IPv4, IPv6 and invalid addresses match PHP's `strlen()` result, including `strlen(false)`.
//! - A value observation (`=== false`, `bin2hex`) keeps the boxed helper.
//! - `--ir-opt=off` keeps the allocating path with the same lengths.

use crate::support::*;

/// Emits the `main` function body for one source fixture.
fn emit_main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_inet_pton_len_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write inet-pton IR fixture");
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

/// Length-only inet_pton observations match PHP and stay heap-clean.
#[test]
fn inet_pton_len_sink_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$s = '127.0.0.1';
echo strlen(inet_pton($s)), '|';
echo strlen(inet_pton('::1')), '|';
echo strlen(inet_pton('not-an-address')), '|';
echo inet_pton('127.0.0.1') === false ? 'bad' : bin2hex(inet_pton('127.0.0.1')), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "4|16|0|7f000001\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// The sink uses the length helper only for the unobserved boxed result.
#[test]
fn inet_pton_len_sink_ir_uses_length_helper() {
    let ir = emit_main_ir(
        r#"<?php
$s = '127.0.0.1';
echo strlen(inet_pton($s)), '|', inet_pton($s);
"#,
    );
    assert!(ir.contains("runtime.inet_pton_len"), "{ir}");
    assert!(ir.contains("runtime.inet_pton "), "{ir}");
}

/// Disabling EIR optimization keeps the boxed call and the same lengths.
#[test]
fn inet_pton_len_sink_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
echo strlen(inet_pton('127.0.0.1')), '|', strlen(inet_pton('::1')), '|';
echo strlen(inet_pton('bad')), "\n";
"#,
        );
        assert_eq!(out, "4|16|0\n");
    });
}
