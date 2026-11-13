//! Purpose:
//! End-to-end coverage for the allocation-free `count(count_chars($s, 1))` sink.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Empty, binary, repeated and full-alphabet inputs compare against PHP.
//! - Mode 0 keeps the real 256-entry tally and its count; only mode 1 is folded.
//! - `--ir-opt=off` keeps the allocating tally with the same counts.

use crate::support::*;

/// Emits the `main` function body for one source fixture.
fn emit_main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_count_chars_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write count-chars IR fixture");
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

/// Distinct-byte counts match PHP and stay heap-clean.
#[test]
fn count_chars_count_sink_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
echo count(count_chars('abcdefgh', 1)), '|';
echo count(count_chars('', 1)), '|';
echo count(count_chars("\x00\x00\xff\xff", 1)), '|';
$s = '';
for ($i = 0; $i < 256; $i++) { $s .= chr($i); }
echo count(count_chars($s, 1)), '|';
echo count(count_chars($s, 0)), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "8|0|2|256|256\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// The sink uses the distinct helper and drops the temporary tally hash.
#[test]
fn count_chars_count_sink_ir_uses_helper() {
    let ir = emit_main_ir(
        r#"<?php
$s = str_repeat('abcdefgh', 16);
echo count(count_chars($s, 1));
"#,
    );
    assert!(ir.contains("runtime.count_chars_distinct"), "{ir}");
    assert!(!ir.contains("runtime.count_chars "), "{ir}");
}

/// Disabling EIR optimization keeps the allocating tally and the same counts.
#[test]
fn count_chars_count_sink_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
echo count(count_chars('abcdefgh', 1)), '|', count(count_chars('', 1)), "\n";
"#,
        );
        assert_eq!(out, "8|0\n");
    });
}
