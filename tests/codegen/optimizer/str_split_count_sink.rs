//! Purpose:
//! End-to-end coverage for the allocation-free `count(str_split(...))` sink.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Hit, empty, binary, two-byte and invalid-length cases compare against PHP.
//! - The IR guard asserts the dedicated helper and the absence of the allocating split.
//! - `--ir-opt=off` keeps the real `str_split()` array and the same count.

use crate::support::*;

/// Emits the `main` function body for one source fixture.
fn emit_main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_str_split_count_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write split-count IR fixture");
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

/// Count-only `str_split()` observations match PHP and stay heap-clean.
#[test]
fn str_split_count_sink_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
echo count(str_split('abcdef', 2)), '|';
echo count(str_split('')), '|';
echo count(str_split('abc', 76)), '|';
echo count(str_split("\x00\xff\x00", 1)), '|';
try { echo count(str_split('abc', 0)); } catch (ValueError $e) { echo 'VE'; }
echo "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "3|0|1|3|VE\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// The sink uses the count helper and drops the allocating split call.
#[test]
fn str_split_count_sink_ir_uses_helper() {
    let ir = emit_main_ir(
        r#"<?php
$s = str_repeat('abcdef', 20);
echo count(str_split($s, 2));
"#,
    );
    assert!(ir.contains("runtime.str_split_count"), "{ir}");
    assert!(!ir.contains("runtime.str_split "), "{ir}");
    assert!(!ir.contains("push_call_operand_owner"), "{ir}");
}

/// Disabling EIR optimization keeps the real array split and the same counts.
#[test]
fn str_split_count_sink_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
echo count(str_split('abcdef', 2)), '|', count(str_split('')), "\n";
"#,
        );
        assert_eq!(out, "3|0\n");
    });
}
