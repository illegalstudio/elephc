//! Purpose:
//! Provides JSON decode error-path tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Syntax, depth, UTF-16, and throw-on-error paths must update runtime JSON error state.

use crate::support::*;

/// Malformed JSON: top-level garbage returns Mixed(null) and sets
/// JSON_ERROR_SYNTAX, matching PHP's json_decode behavior.
/// Verifies malformed JSON ("not json") returns NULL and sets JSON_ERROR_SYNTAX (4).
#[test]
fn test_json_decode_garbage_returns_null_and_syntax_error() {
    let out = compile_and_run(
        r#"<?php
            $r = json_decode("not json");
            echo gettype($r) . "|" . json_last_error();
        "#,
    );
    assert_eq!(out, "NULL|4");
}

/// Verifies empty input returns NULL and sets JSON_ERROR_SYNTAX (4).
#[test]
fn test_json_decode_empty_input_returns_null_and_syntax_error() {
    let out = compile_and_run(
        r#"<?php
            $r = json_decode("");
            echo gettype($r) . "|" . json_last_error();
        "#,
    );
    assert_eq!(out, "NULL|4");
}

/// Verifies a truncated object ("{") returns NULL and sets a non-zero error.
#[test]
fn test_json_decode_unclosed_object_sets_error() {
    // Truncated container input fails the validator and returns null.
    let out = compile_and_run(
        r#"<?php
            $r = json_decode("{");
            echo gettype($r) . "|" . (json_last_error() != 0 ? "err" : "ok");
        "#,
    );
    assert_eq!(out, "NULL|err");
}

/// Verifies a truncated array ("[1,2,3") returns NULL and sets a non-zero error.
#[test]
fn test_json_decode_unclosed_array_sets_error() {
    let out = compile_and_run(
        r#"<?php
            $r = json_decode("[1,2,3");
            echo gettype($r) . "|" . (json_last_error() != 0 ? "err" : "ok");
        "#,
    );
    assert_eq!(out, "NULL|err");
}

/// Verifies JSON_THROW_ON_ERROR causes a JsonException with "Syntax error" on malformed input.
#[test]
fn test_json_decode_throws_on_invalid_with_throw_flag() {
    let out = compile_and_run(
        r#"<?php
            try {
                json_decode("not json", null, 512, JSON_THROW_ON_ERROR);
                echo "no-throw";
            } catch (JsonException $e) {
                echo "caught:" . $e->getMessage();
            }
        "#,
    );
    assert_eq!(out, "caught:Syntax error");
}

/// Verifies exceeding the depth limit returns NULL and sets JSON_ERROR_DEPTH (1).
#[test]
fn test_json_decode_depth_limit_returns_null_and_depth_error() {
    let out = compile_and_run(
        r#"<?php
            $deep = str_repeat("[", 200) . "1" . str_repeat("]", 200);
            $r = json_decode($deep, true, 50);
            echo gettype($r) . "|" . json_last_error();
        "#,
    );
    assert_eq!(out, "NULL|1");
}

/// Verifies JSON_THROW_ON_ERROR raises JsonException with "Maximum stack depth exceeded"
/// when depth limit is exceeded.
#[test]
fn test_json_decode_throws_on_depth_overflow() {
    let out = compile_and_run(
        r#"<?php
            $deep = str_repeat("[", 200) . "1" . str_repeat("]", 200);
            try {
                json_decode($deep, true, 50, JSON_THROW_ON_ERROR);
                echo "no-throw";
            } catch (JsonException $e) {
                echo "caught:" . $e->getMessage();
            }
        "#,
    );
    assert_eq!(out, "caught:Maximum stack depth exceeded");
}

/// Verifies a successful follow-up call resets error state after a prior failure.
#[test]
fn test_json_decode_resets_error_on_success() {
    // A previous failure should not bleed into a successful follow-up call.
    let out = compile_and_run(
        r#"<?php
            json_decode("not json");
            $code = json_last_error();
            json_decode("42");
            $code2 = json_last_error();
            echo $code . "|" . $code2;
        "#,
    );
    assert_eq!(out, "4|0");
}

/// Verifies malformed values inside the decoder ([1,], {"a":1,}, 01, truex, [1]x) each
/// return NULL and set JSON_ERROR_SYNTAX (4). Merges 5 cases into one test.
#[test]
fn test_json_decode_rejects_malformed_values_inside_decoder() {
    let out = compile_and_run(
        r#"<?php
            $cases = [
                "[1,]",
                "{\"a\":1,}",
                "01",
                "truex",
                "[1]x",
            ];

            foreach ($cases as $json) {
                $r = json_decode($json);
                echo gettype($r) . ":" . json_last_error() . "\n";
            }
        "#,
    );
    assert_eq!(out, "NULL:4\nNULL:4\nNULL:4\nNULL:4\nNULL:4\n");
}

/// Verifies json_last_error_msg returns "Syntax error" after a decode failure.
#[test]
fn test_json_decode_last_error_msg_after_failure() {
    let out = compile_and_run(
        r#"<?php
            json_decode("");
            echo json_last_error_msg();
        "#,
    );
    assert_eq!(out, "Syntax error");
}

// JSON_ERROR_UTF16 (10) — lone UTF-16 surrogate detection.
//
// Per RFC 8259 §7 and PHP semantics, a `\uXXXX` escape in the high-surrogate
// range (0xD800..0xDBFF) must be immediately followed by a `\uYYYY` escape in
// the low-surrogate range (0xDC00..0xDFFF). A lone high or low surrogate
// raises JSON_ERROR_UTF16 with the message "Single unpaired UTF-16 surrogate
// in unicode escape".

/// Verifies a lone high surrogate (\uD83D) sets JSON_ERROR_UTF16 (10).
#[test]
fn test_json_decode_lone_high_surrogate_sets_utf16_error() {
    let out = compile_and_run(
        r#"<?php json_decode("\"\\uD83D\""); echo json_last_error();"#,
    );
    assert_eq!(out, "10");
}

/// Verifies a lone low surrogate (\uDE00) sets JSON_ERROR_UTF16 (10).
#[test]
fn test_json_decode_lone_low_surrogate_sets_utf16_error() {
    let out = compile_and_run(
        r#"<?php json_decode("\"\\uDE00\""); echo json_last_error();"#,
    );
    assert_eq!(out, "10");
}

/// Verifies a high surrogate followed by a non-low-surrogate escape (\uD83D\u0041) sets
/// JSON_ERROR_UTF16 (10).
#[test]
fn test_json_decode_high_followed_by_non_low_sets_utf16_error() {
    let out = compile_and_run(
        r#"<?php json_decode("\"\\uD83D\\u0041\""); echo json_last_error();"#,
    );
    assert_eq!(out, "10");
}

/// Verifies a high surrogate followed by a non-escape character (\uD83Dx) sets
/// JSON_ERROR_UTF16 (10).
#[test]
fn test_json_decode_high_followed_by_non_escape_sets_utf16_error() {
    let out = compile_and_run(
        r#"<?php json_decode("\"\\uD83Dx\""); echo json_last_error();"#,
    );
    assert_eq!(out, "10");
}

/// Verifies a truncated low surrogate (\uD83D\u) sets JSON_ERROR_UTF16 (10) — the high
/// surrogate already triggered the pair handshake and the truncation occurs before the
/// surrogate range check, but the observed behavior is UTF16.
#[test]
fn test_json_decode_truncated_high_surrogate_sets_utf16_error() {
    let out = compile_and_run(
        r#"<?php json_decode("\"\\uD83D\\u\""); echo json_last_error();"#,
    );
    // Truncated low-surrogate \u — the validator rejects on syntax (no hex
    // digits) before reaching the surrogate range check. Either UTF16 (10)
    // or SYNTAX (4) would be acceptable from PHP's perspective; lock the
    // current behavior of UTF16 since the high surrogate already triggered
    // the surrogate-pair handshake.
    assert_eq!(out, "10");
}

/// Verifies a valid surrogate pair (\uD83D\uDE00 = 😀) decodes without error; error=0 and
/// the 4-byte UTF-8 result round-trips correctly.
#[test]
fn test_json_decode_valid_surrogate_pair_no_error() {
    let out = compile_and_run(
        r#"<?php $x = json_decode("\"\\uD83D\\uDE00\""); echo json_last_error() . ":" . $x;"#,
    );
    // U+1F600 is "😀" (4-byte UTF-8). Verify error is 0 and the bytes round-trip.
    assert_eq!(out, "0:\u{1F600}");
}

/// Verifies JSON_THROW_ON_ERROR raises JsonException with "Single unpaired UTF-16 surrogate
/// in unicode escape" when a lone high surrogate is encountered.
#[test]
fn test_json_decode_lone_high_with_throw_flag_throws() {
    let out = compile_and_run(
        r#"<?php try { json_decode("\"\\uD83D\"", null, 512, JSON_THROW_ON_ERROR); echo "no throw"; } catch (JsonException $e) { echo "thrown:" . $e->getMessage(); }"#,
    );
    assert_eq!(
        out,
        "thrown:Single unpaired UTF-16 surrogate in unicode escape"
    );
}

// JSON_ERROR_DEPTH (1) — PHP rejects when active nesting depth equals the
// user-supplied $depth (strict comparison). A flat array `[1]` has 1
// container level, so json_decode rejects it at depth=1 and accepts it at
// depth=2. PHP encode (separately) uses non-strict semantics; only decode
// and validate apply this rule.

/// Verifies json_decode rejects a flat array at depth=1 (strict comparison: active==limit → fail).
#[test]
fn test_json_decode_flat_array_depth_one_fails() {
    let out = compile_and_run(
        r#"<?php json_decode("[1]", false, 1); echo json_last_error();"#,
    );
    assert_eq!(out, "1");
}

/// Verifies json_decode accepts a flat array at depth=2 (active=1 < limit=2 → pass).
#[test]
fn test_json_decode_flat_array_depth_two_succeeds() {
    let out = compile_and_run(
        r#"<?php $x = json_decode("[1]", false, 2); echo json_last_error() . ":" . count($x);"#,
    );
    assert_eq!(out, "0:1");
}

/// Verifies json_decode rejects nested arrays at depth=2 ([[1]] has active=2 == limit=2).
#[test]
fn test_json_decode_nested_array_depth_two_fails() {
    let out = compile_and_run(
        r#"<?php json_decode("[[1]]", false, 2); echo json_last_error();"#,
    );
    assert_eq!(out, "1");
}

/// Verifies json_decode accepts nested arrays at depth=3 (active=2 < limit=3).
#[test]
fn test_json_decode_nested_array_depth_three_succeeds() {
    let out = compile_and_run(
        r#"<?php json_decode("[[1]]", false, 3); echo json_last_error();"#,
    );
    assert_eq!(out, "0");
}

/// Verifies json_decode rejects an object at depth=1 (object is one container level).
#[test]
fn test_json_decode_object_depth_one_fails() {
    let out = compile_and_run(
        r#"<?php json_decode("{\"a\":1}", false, 1); echo json_last_error();"#,
    );
    assert_eq!(out, "1");
}

/// Verifies json_decode accepts a scalar at depth=1 (scalars never enter a container).
#[test]
fn test_json_decode_scalar_depth_one_succeeds() {
    let out = compile_and_run(
        r#"<?php $x = json_decode("42", false, 1); echo json_last_error() . ":" . $x;"#,
    );
    assert_eq!(out, "0:42");
}
