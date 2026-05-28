//! Purpose:
//! Provides JSON control-character escaping tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - String and key escaping must be byte-faithful for JSON control bytes.

use super::*;

// JSON requires every control byte (< 0x20) inside a string to be escaped.
// elefphc's encoder uses the canonical short escapes for the five RFC-named
// controls (\b, \t, \n, \f, \r) and emits \uXXXX for every other control
// byte. The malformed inputs are constructed via chr() since elefphc's
// lexer does not parse \xHH string escapes.

/// Verifies per-byte escape dispatch for every distinct control-byte class.
///
/// Exercises:
///   * 0x00..0x07 / 0x0B / 0x0E..0x1F → generic unicode escape (\uXXXX)
///   * 0x08 (\b) and 0x0C (\f) → canonical short escapes
///
/// The mixed-byte test below still verifies that short and unicode escapes
/// can interleave in one string. Inputs constructed via chr() because
/// elefphc's lexer does not parse \xHH string escapes.
#[test]
fn test_json_encode_control_byte_dispatch() {
    let out = compile_and_run(
        r#"<?php
echo json_encode("a" . chr(0x00) . "b") . "\n";
echo json_encode("a" . chr(0x01) . "b") . "\n";
echo json_encode("a" . chr(0x08) . "b") . "\n";
echo json_encode("a" . chr(0x0B) . "b") . "\n";
echo json_encode("a" . chr(0x0C) . "b") . "\n";
echo json_encode("a" . chr(0x0E) . "b") . "\n";
echo json_encode("a" . chr(0x1F) . "b");
"#,
    );
    let expected = "\"a\\u0000b\"\n\"a\\u0001b\"\n\"a\\bb\"\n\"a\\u000Bb\"\n\"a\\fb\"\n\"a\\u000Eb\"\n\"a\\u001Fb\"";
    assert_eq!(out, expected);
}

/// Verifies that 0x20 (space) is NOT treated as a control character and
/// is copied as-is into the JSON output. This is the first byte above
/// the control-byte range (< 0x20) that requires no escaping.
#[test]
fn test_json_encode_space_byte_remains_literal() {
    let out = compile_and_run(
        r#"<?php echo json_encode("a" . chr(0x20) . "b");"#,
    );
    assert_eq!(out, r#""a b""#);
}

/// Verifies interleaving of canonical short escapes (\b, \t, \n) and
/// generic \u00XX escapes in a single JSON string, confirming the
/// dispatch handles both branches.
#[test]
fn test_json_encode_multiple_control_bytes() {
    let out = compile_and_run(
        r#"<?php echo json_encode(chr(0x00) . chr(0x08) . chr(0x09) . chr(0x0A) . chr(0x0B));"#,
    );
    assert_eq!(out, "\"\\u0000\\b\\t\\n\\u000B\"");
}

/// Verifies control-byte escaping works for array element strings.
/// The dispatcher routes through __rt_json_encode_str for every array
/// element, so escaping must apply uniformly.
#[test]
fn test_json_encode_control_byte_inside_array() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["pre", chr(0x01), "post"]);"#,
    );
    assert_eq!(out, "[\"pre\",\"\\u0001\",\"post\"]");
}

/// Verifies control-byte escaping in assoc-array value strings.
#[test]
fn test_json_encode_control_byte_in_assoc_value() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["k" => "x" . chr(0x07) . "y"]);"#,
    );
    assert_eq!(out, "{\"k\":\"x\\u0007y\"}");
}

/// Verifies a tab (0x09) inside an assoc key is escaped as \t in JSON output.
#[test]
fn test_json_encode_tab_in_assoc_key() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["k" . chr(0x09) . "v" => 1]);"#,
    );
    assert_eq!(out, "{\"k\\tv\":1}");
}

/// Verifies a double-quote inside an assoc key is escaped as \" in JSON output.
#[test]
fn test_json_encode_quote_in_assoc_key() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["a\"b" => 1]);"#,
    );
    assert_eq!(out, "{\"a\\\"b\":1}");
}

/// Verifies a control byte (0x01) inside an assoc key is escaped as \u00XX in JSON output.
#[test]
fn test_json_encode_control_byte_in_assoc_key() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["a" . chr(0x01) . "b" => 2]);"#,
    );
    assert_eq!(out, "{\"a\\u0001b\":2}");
}

/// Verifies a backslash inside an assoc key is escaped as \\ in JSON output.
#[test]
fn test_json_encode_backslash_in_assoc_key() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["a\\b" => 3]);"#,
    );
    assert_eq!(out, "{\"a\\\\b\":3}");
}

/// Verifies integer assoc keys are formatted as decimal digits and do not
/// require escaping. Regression test for the assoc-key refactor.
#[test]
fn test_json_encode_integer_assoc_key_unchanged() {
    let out = compile_and_run(
        r#"<?php echo json_encode([42 => "x"]);"#,
    );
    assert_eq!(out, r#"{"42":"x"}"#);
}
