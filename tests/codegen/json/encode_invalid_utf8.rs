//! Purpose:
//! Provides JSON encode malformed UTF-8 tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Invalid UTF-8 flags must select error, ignore, substitute, or throw behavior.

use super::*;

// __rt_json_encode_str validates every multibyte UTF-8 byte before emitting
// the codepoint. Lone continuation bytes, truncated multi-byte sequences,
// and invalid lead bytes (0xC0/0xC1 overlong starts, 0xF5+ out-of-range)
// route to the malformed handler which honors the JSON_INVALID_UTF8_*
// flags. The bytes used to construct malformed inputs are produced via
// chr() since elephc's lexer does not parse \xHH escapes.

/// Tests that a lone continuation byte (0x80) in input string causes
/// json_encode() to return false when no sanitization flag is set.
#[test]
fn test_json_encode_lone_continuation_default_returns_false() {
    let out = compile_and_run(
        r#"<?php echo json_encode("a" . chr(0x80) . "b") === false ? "false" : "json";"#,
    );
    assert_eq!(out, "false");
}

/// Tests that json_last_error() returns JSON_ERROR_UTF8 (5) after encoding
/// a string containing a lone continuation byte with no sanitization flag.
#[test]
fn test_json_encode_lone_continuation_default_sets_error_code() {
    let out = compile_and_run(
        r#"<?php
json_encode("a" . chr(0x80) . "b");
echo json_last_error();
"#,
    );
    assert_eq!(out, "5");
}

/// Tests that json_last_error_msg() produces the standard PHP malformed-UTF-8
/// message after encoding a string with a lone continuation byte.
#[test]
fn test_json_encode_lone_continuation_default_sets_error_msg() {
    let out = compile_and_run(
        r#"<?php
json_encode("a" . chr(0x80) . "b");
echo json_last_error_msg();
"#,
    );
    assert_eq!(
        out,
        "Malformed UTF-8 characters, possibly incorrectly encoded"
    );
}

/// Tests that JSON_INVALID_UTF8_IGNORE drops the lone continuation byte from
/// the encoded output and does NOT set JSON_ERROR_UTF8.
#[test]
fn test_json_encode_invalid_utf8_ignore_silences_error() {
    // JSON_INVALID_UTF8_IGNORE drops the malformed bytes WITHOUT setting
    // JSON_ERROR_UTF8.
    let out = compile_and_run(
        r#"<?php
$encoded = json_encode("a" . chr(0x80) . "b", JSON_INVALID_UTF8_IGNORE);
echo $encoded . "/" . json_last_error();
"#,
    );
    assert_eq!(out, r#""ab"/0"#);
}

/// Tests that JSON_INVALID_UTF8_SUBSTITUTE replaces the lone continuation byte
/// with the Unicode replacement character U+FFFD (emitted as \\uFFFD escape
/// because JSON_UNESCAPED_UNICODE is clear).
#[test]
fn test_json_encode_invalid_utf8_substitute_emits_replacement() {
    // JSON_INVALID_UTF8_SUBSTITUTE replaces malformed bytes with the
    // 6-byte escape � because JSON_UNESCAPED_UNICODE is clear.
    let out = compile_and_run(
        r#"<?php
$encoded = json_encode("a" . chr(0x80) . "b", JSON_INVALID_UTF8_SUBSTITUTE);
echo $encoded . "/" . json_last_error();
"#,
    );
    assert_eq!(out, "\"a\\uFFFDb\"/0");
}

/// Tests that a truncated 2-byte UTF-8 sequence (valid lead 0xC3 with no
/// continuation byte) causes json_encode() to return false with no flag.
#[test]
fn test_json_encode_truncated_two_byte_default_returns_false() {
    // chr(0xC3) is a valid 2-byte lead but no continuation byte follows
    // before the end of input. The bounds check inside utf8_2 routes this
    // to the malformed handler, causing the wrapper to return false.
    let out = compile_and_run(
        r#"<?php echo json_encode("a" . chr(0xC3)) === false ? "false" : "json";"#,
    );
    assert_eq!(out, "false");
}

/// Tests that a truncated 2-byte UTF-8 sequence is replaced by the Unicode
/// replacement character when JSON_INVALID_UTF8_SUBSTITUTE is set.
#[test]
fn test_json_encode_truncated_two_byte_substitute_emits_replacement() {
    let out = compile_and_run(
        r#"<?php
$encoded = json_encode("a" . chr(0xC3), JSON_INVALID_UTF8_SUBSTITUTE);
echo $encoded;
"#,
    );
    assert_eq!(out, "\"a\\uFFFD\"");
}

/// Tests that an out-of-range lead byte (0xFF, above 0xF5 RFC 3629 limit)
/// causes json_encode() to return false with no flag.
#[test]
fn test_json_encode_invalid_lead_byte_default_returns_false() {
    // chr(0xFF) is in the 0xF5..0xFF range that RFC 3629 forbids - the
    // dispatcher's lead-byte gate routes it straight to the malformed
    // handler.
    let out = compile_and_run(
        r#"<?php echo json_encode("x" . chr(0xFF) . "y") === false ? "false" : "json";"#,
    );
    assert_eq!(out, "false");
}

/// Tests that an out-of-range lead byte (0xFF) is replaced by the Unicode
/// replacement character when JSON_INVALID_UTF8_SUBSTITUTE is set.
#[test]
fn test_json_encode_invalid_lead_byte_substitute_emits_replacement() {
    let out = compile_and_run(
        r#"<?php
$encoded = json_encode("x" . chr(0xFF) . "y", JSON_INVALID_UTF8_SUBSTITUTE);
echo $encoded;
"#,
    );
    assert_eq!(out, "\"x\\uFFFDy\"");
}

/// Tests that a valid 2-byte lead (0xC3) followed by a non-continuation byte
/// ('A' = 0x41) causes json_encode() to return false with no flag.
#[test]
fn test_json_encode_invalid_continuation_default_returns_false() {
    // chr(0xC3) chr('A') - valid 2-byte lead followed by a non-continuation
    // byte. The continuation validation in utf8_2 catches this and skips
    // exactly one byte internally, then the wrapper returns false because
    // the call reported JSON_ERROR_UTF8 without a sanitization flag.
    let out = compile_and_run(
        r#"<?php echo json_encode("z" . chr(0xC3) . "A") === false ? "false" : "json";"#,
    );
    assert_eq!(out, "false");
}

/// Tests that JSON_THROW_ON_ERROR raises JsonException with the standard
/// malformed-UTF-8 message when the input contains a lone continuation byte.
#[test]
fn test_json_encode_malformed_throw_on_error_raises_exception() {
    let out = compile_and_run(
        r#"<?php
try {
    json_encode("bad" . chr(0x80) . "input", JSON_THROW_ON_ERROR);
    echo "no throw";
} catch (JsonException $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Malformed UTF-8 characters, possibly incorrectly encoded"
    );
}

/// Tests that a JsonException raised for malformed UTF-8 is caught by a
/// RuntimeException catch clause (JsonException extends RuntimeException).
#[test]
fn test_json_encode_malformed_caught_as_runtime_exception() {
    // JsonException extends RuntimeException, so a RuntimeException catch
    // clause must catch the malformed-UTF-8 throw too.
    let out = compile_and_run(
        r#"<?php
try {
    json_encode("oops" . chr(0xC0), JSON_THROW_ON_ERROR);
} catch (RuntimeException $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Malformed UTF-8 characters, possibly incorrectly encoded"
    );
}

/// Tests that JSON_INVALID_UTF8_SUBSTITUTE applies to malformed bytes inside
/// array element strings, replacing them with U+FFFD in the encoded output.
#[test]
fn test_json_encode_substitute_inside_array() {
    // The dispatcher routes through __rt_json_encode_str for every array
    // element string, so the substitute behavior must apply uniformly.
    let out = compile_and_run(
        r#"<?php
echo json_encode(
    ["ok", "bad" . chr(0x80) . "byte", "fine"],
    JSON_INVALID_UTF8_SUBSTITUTE
);
"#,
    );
    assert_eq!(out, "[\"ok\",\"bad\\uFFFDbyte\",\"fine\"]");
}

/// Tests that JSON_INVALID_UTF8_IGNORE drops malformed bytes inside array
/// element strings from the encoded output.
#[test]
fn test_json_encode_ignore_inside_array() {
    let out = compile_and_run(
        r#"<?php
echo json_encode(
    ["pre", "x" . chr(0xFF) . "y"],
    JSON_INVALID_UTF8_IGNORE
);
"#,
    );
    assert_eq!(out, r#"["pre","xy"]"#);
}

/// Tests that JSON_INVALID_UTF8_SUBSTITUTE is a no-op on well-formed ASCII
/// input — the output must be identical to json_encode() with no flag.
#[test]
fn test_json_encode_clean_input_unaffected_by_substitute_flag() {
    // Sanitization flags must be no-ops on well-formed input - clean
    // ASCII strings still come out exactly the same.
    let out = compile_and_run(
        r#"<?php echo json_encode("hello", JSON_INVALID_UTF8_SUBSTITUTE);"#,
    );
    assert_eq!(out, r#""hello""#);
}

/// Tests that JSON_INVALID_UTF8_SUBSTITUTE is a no-op on well-formed multibyte
/// UTF-8 (U+00E9 = 0xC3 0xA9) — the é is correctly escaped as \\u00E9.
#[test]
fn test_json_encode_clean_multibyte_unaffected_by_substitute_flag() {
    // Well-formed multibyte UTF-8 (here e-acute = 0xC3 0xA9) passes
    // the dispatcher validation and is escaped as é because
    // JSON_UNESCAPED_UNICODE is clear.
    let out = compile_and_run(
        r#"<?php echo json_encode("caf" . chr(0xC3) . chr(0xA9), JSON_INVALID_UTF8_SUBSTITUTE);"#,
    );
    assert_eq!(out, "\"caf\\u00E9\"");
}
