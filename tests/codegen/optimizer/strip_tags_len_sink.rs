//! Purpose:
//! End-to-end coverage for the allocation-free `strlen(strip_tags(...))` sink.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Plain, allow-list, array allow-list, comment/PHP-tag, and NUL inputs compare
//!   against PHP 8.5 lengths while the emitted string path stays available.
//! - Argument evaluation order, user-visible callbacks, and strict allow-list
//!   `TypeError`s survive the rewrite.
//! - `--ir-opt=off` keeps the allocating scanner with the same output.

use crate::support::*;

/// Emits the `main` function body for one source fixture.
fn emit_main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_strip_tags_len_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write strip-tags IR fixture");
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

/// Length-only strip_tags observations match PHP and stay heap-clean.
#[test]
fn strip_tags_len_sink_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
declare(strict_types=1);
$s = "<p>Hello <b>world</b> <a href=\"x\">link</a></p>";
echo strlen(strip_tags($s)), '|';
echo strlen(strip_tags($s, '<b>')), '|';
echo strlen(strip_tags($s, ['p'])), '|';
$t = "a<!-- hide -->b<?php echo 1; ?>c\0d<em>e</em>";
echo strlen(strip_tags($t)), '|';
echo strip_tags($s, '<b>'), "\n";
try { echo strlen(strip_tags('<b>x</b>', 1)); } catch (TypeError $e) { echo 'TE'; }
echo "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "16|23|23|5|Hello <b>world</b> link\nTE\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// The count helper still evaluates the subject and allow-list exactly once.
#[test]
fn strip_tags_len_sink_preserves_argument_effects() {
    let out = compile_and_run(
        r#"<?php
function subject(): string { echo 'S'; return '<b>x</b>'; }
function allowed(): string { echo 'A'; return '<b>'; }
echo strlen(strip_tags(subject(), allowed())), "\n";
"#,
    );
    assert_eq!(out, "SA8\n");
}

/// The sink uses the count mode and leaves non-length calls on the string path.
#[test]
fn strip_tags_len_sink_ir_uses_count_mode() {
    let ir = emit_main_ir(
        r#"<?php
$s = '<p>Hello <b>world</b></p>';
echo strlen(strip_tags($s)), '|', strip_tags($s);
"#,
    );
    assert!(ir.contains("runtime.strip_tags_len"), "{ir}");
    assert!(ir.contains("runtime.strip_tags "), "{ir}");
}

/// Disabling EIR optimization keeps the allocating scanner and the same output.
#[test]
fn strip_tags_len_sink_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
$s = '<p>Hello <b>world</b></p>';
echo strlen(strip_tags($s)), '|', strlen(strip_tags($s, '<b>')), "\n";
"#,
        );
        assert_eq!(out, "11|18\n");
    });
}
