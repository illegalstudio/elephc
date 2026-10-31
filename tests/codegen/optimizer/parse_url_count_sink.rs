//! Purpose:
//! End-to-end coverage for the allocation-free `count(parse_url(...))` sink.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Full-array counts, empty/relative URLs and invalid-URL `TypeError` behavior match PHP.
//! - A component selector keeps the real parse_url result and the count TypeError path.
//! - `--ir-opt=off` keeps the allocating parser with the same observable behavior.

use crate::support::*;

/// Emits the `main` function body for one source fixture.
fn emit_main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_parse_url_count_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write parse-url IR fixture");
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

/// Count-only parse_url observations match PHP and stay heap-clean.
#[test]
fn parse_url_count_sink_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$u = "https://user:pass@example.com:8080/path?q=1#frag";
echo count(parse_url($u)), '|';
echo count(parse_url('http://example.com')), '|';
echo count(parse_url('/path?q=1')), '|';
try { echo count(parse_url('http://')); } catch (TypeError $e) { echo 'TE'; }
echo "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "8|2|2|TE\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// The sink uses the count helper and drops the temporary parsed array.
#[test]
fn parse_url_count_sink_ir_uses_helper() {
    let ir = emit_main_ir(
        r#"<?php
$u = 'https://example.com/path?q=1';
echo count(parse_url($u));
"#,
    );
    assert!(ir.contains("runtime.parse_url_count"), "{ir}");
    assert!(!ir.contains("runtime.parse_url "), "{ir}");
}

/// Disabling EIR optimization keeps the allocating parser and the same behavior.
#[test]
fn parse_url_count_sink_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
$u = 'https://example.com/path?q=1';
echo count(parse_url($u)), '|', count(parse_url('http://example.com')), "\n";
"#,
        );
        assert_eq!(out, "4|2\n");
    });
}
