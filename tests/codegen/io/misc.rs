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
