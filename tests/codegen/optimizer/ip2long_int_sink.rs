//! Purpose:
//! End-to-end coverage for the raw `(int)ip2long(...)` specialization.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Valid addresses keep PHP's 32-bit integer; invalid addresses become the zero
//!   that `(int)false` yields.
//! - Any value observation keeps the boxed `int|false` result.
//! - `--ir-opt=off` keeps the boxed path with the same integers.

use crate::support::*;

/// Emits the `main` function body for one source fixture.
fn emit_main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_ip2long_int_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write ip2long IR fixture");
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

/// Integer-observed ip2long results match PHP and stay heap-clean.
#[test]
fn ip2long_int_sink_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
echo (int)ip2long('192.168.1.42'), '|';
echo (int)ip2long('not-an-address'), '|';
echo (int)ip2long('255.255.255.255'), '|';
var_export(ip2long('not-an-address'));
echo "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "3232235818|0|4294967295|false\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// The raw helper is used only when every observation casts the result to int.
#[test]
fn ip2long_int_sink_ir_uses_raw_helper() {
    let ir = emit_main_ir(
        r#"<?php
$s = '192.168.1.42';
echo (int)ip2long($s), '|', ip2long($s);
"#,
    );
    assert!(ir.contains("runtime.ip2long_int"), "{ir}");
    assert!(ir.contains("runtime.ip2long "), "{ir}");
}

/// Disabling EIR optimization keeps the boxed call and the same integers.
#[test]
fn ip2long_int_sink_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
echo (int)ip2long('192.168.1.42'), '|', (int)ip2long('bad'), "\n";
"#,
        );
        assert_eq!(out, "3232235818|0\n");
    });
}
