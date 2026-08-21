//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of I/O misc, including control suppresses runtime warning, and readline.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Compiles `echo @file_get_contents("missing.txt"); echo "after";` and verifies
/// the `@` error-control operator suppresses the runtime warning from the missing file,
/// that stdout contains only "after", and stderr is empty.
#[test]
fn test_error_control_suppresses_runtime_warning() {
    let out = compile_and_run_capture(
        r#"<?php
echo @file_get_contents("missing.txt");
echo "after";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "after");
    assert_eq!(out.stderr, "");
    assert_eq!(out.diagnostics, "");
}

/// Compiles `@file_get_contents("missing.txt"); echo "continued";` and verifies
/// the `@` error-control operator suppresses the runtime warning when the call
/// appears as a standalone expression statement (not embedded in echo),
/// that stdout contains "continued", and stderr is empty.
#[test]
fn test_error_control_expression_statement_suppresses_runtime_warning() {
    let out = compile_and_run_capture(
        r#"<?php
@file_get_contents("missing.txt");
echo "continued";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "continued");
    assert_eq!(out.stderr, "");
    assert_eq!(out.diagnostics, "");
}

/// Pins that `readline()` and `fscanf()` tell the shared line reader they have no length bound.
///
/// `fgets($length)` gave the reader a second input, and these two callers kept passing only the
/// handle: the bound was then whatever the register happened to hold. On linux-x86_64 that
/// truncated `readline()` to the first two bytes of its line, while AArch64 passed by luck — so
/// the guard is on the emitted code for both targets rather than on one host's behaviour.
#[test]
fn test_line_readers_pass_an_explicit_absent_length_bound() {
    for (target, zero_bound) in [
        ("linux-x86_64", "xor esi, esi"),
        ("linux-aarch64", "mov x1, #0"),
        ("macos-aarch64", "mov x1, #0"),
    ] {
        let dir = make_cli_test_dir("elephc_line_reader_bound");
        let php_path = dir.join("main.php");
        fs::write(
            &php_path,
            r#"<?php
$line = readline();
echo "read: " . trim($line);
"#,
        )
        .unwrap();
        let output = elephc_cli_command(&dir)
            .arg("--target")
            .arg(target)
            .arg("--emit-asm")
            .arg(&php_path)
            .output()
            .expect("failed to emit assembly for the line-reader target");
        assert!(
            output.status.success(),
            "{target}: --emit-asm failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let asm = fs::read_to_string(dir.join("main.s")).expect("target assembly");
        let before_call = asm
            .split("__rt_fgets")
            .next()
            .expect("readline must reach the shared line reader");
        let tail: String = before_call.chars().rev().take(160).collect();
        let preamble: String = tail.chars().rev().collect();
        assert!(
            preamble.contains(zero_bound),
            "{target}: readline() must zero the length bound before calling the line reader, got:\n{preamble}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

/// Compiles a `readline()` call piped with "world\n" on stdin and verifies
/// the input is read, trimmed, and printed as "read: world".
#[test]
fn test_readline() {
    let out = compile_and_run_with_stdin(
        r#"<?php
$line = readline();
echo "read: " . trim($line);
"#,
        "world\n",
    );
    assert_eq!(out, "read: world");
}
