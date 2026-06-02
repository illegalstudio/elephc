//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of strings misc, including escaped dollar.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests that a backslash-escaped dollar (`\$`) inside a double-quoted string is
/// treated as a literal dollar sign, matching PHP's escape sequence behavior.
#[test]
fn test_string_escaped_dollar() {
    let out = compile_and_run(r#"<?php echo "price is \$5";"#);
    assert_eq!(out, "price is $5");
}

/// Tests that a multibyte UTF-8 string literal preceding ASCII digits round-trips
/// correctly through the compiler with no byte-level corruption or digit mishandling.
#[test]
fn test_multibyte_string_literal_before_ascii_digits_round_trips() {
    let out = compile_and_run("<?php echo '日本語123';");
    assert_eq!(out, "日本語123");
}

#[test]
fn test_string_control_escape_sequences() {
    // \r, \v, \e, \f process to their ASCII control bytes (regression for
    // the lexer previously emitting a literal backslash for these escapes).
    let out = compile_and_run(
        r#"<?php echo strlen("a\r\nb") . "|" . ord("\r") . "|" . ord("\v") . "|" . ord("\e") . "|" . ord("\f");"#,
    );
    assert_eq!(out, "4|13|11|27|12");
}

// --- md5 / sha1 ---
