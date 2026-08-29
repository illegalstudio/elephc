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
///
/// The `trim()` here is why this test did not see that `readline` used to keep
/// the trailing newline: trimming removes it either way. What the line actually
/// returns is pinned by `readline_strips_the_newline_and_answers_false_at_eof`
/// below, without a trim in the way.
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

/// The three answers `readline()` has, told apart.
///
/// PHP returns the line WITHOUT its trailing newline, `""` for a line the user
/// left empty, and `false` at end of input. elephc used to answer all three with
/// a string — `"abc\n"`, `"\n"` and `""` — so a program could not tell an empty
/// line from the end of the input, and `while (($l = readline()) !== false)`
/// never terminated.
///
/// The order inside the fix is the whole of it, which is why the empty line is
/// asserted alongside EOF: `readline` strips the newline, so stripping BEFORE the
/// end-of-input test would turn a line the user typed into zero bytes and report
/// EOF for it. Measured against `php -n`, one call per input.
///
/// Lengths are printed rather than the values, because the difference between
/// `""` and `false` is invisible when echoed, and so is a trailing newline.
#[test]
fn readline_strips_the_newline_and_answers_false_at_eof() {
    let source = r#"<?php
$one = readline();
echo $one === false ? "false" : "str" . strlen($one);
echo ":";
$two = readline();
echo $two === false ? "false" : "str" . strlen($two);
"#;
    // A terminated line, then an empty one: the newline goes, the empty line is
    // still a line.
    assert_eq!(compile_and_run_with_stdin(source, "abc\n\n"), "str3:str0");
    // A terminated line, then nothing at all: that second read is the end.
    assert_eq!(compile_and_run_with_stdin(source, "abc\n"), "str3:false");
    // Nothing at all, twice: end of input does not become an empty string.
    assert_eq!(compile_and_run_with_stdin(source, ""), "false:false");
    // An unterminated last line keeps every byte it has.
    assert_eq!(compile_and_run_with_stdin(source, "abc"), "str3:false");
    // Exactly one `\n` is removed; a `\r` before it belongs to the line, and php
    // answers `string(4)` for this input.
    assert_eq!(compile_and_run_with_stdin(source, "abc\r\n"), "str4:false");
}
