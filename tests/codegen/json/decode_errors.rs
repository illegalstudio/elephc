use crate::support::*;

// Malformed JSON: top-level garbage returns Mixed(null) and sets
// JSON_ERROR_SYNTAX, matching PHP's json_decode behavior.
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

#[test]
fn test_json_decode_lone_high_surrogate_sets_utf16_error() {
    let out = compile_and_run(
        r#"<?php json_decode("\"\\uD83D\""); echo json_last_error();"#,
    );
    assert_eq!(out, "10");
}

#[test]
fn test_json_decode_lone_low_surrogate_sets_utf16_error() {
    let out = compile_and_run(
        r#"<?php json_decode("\"\\uDE00\""); echo json_last_error();"#,
    );
    assert_eq!(out, "10");
}

#[test]
fn test_json_decode_high_followed_by_non_low_sets_utf16_error() {
    let out = compile_and_run(
        r#"<?php json_decode("\"\\uD83D\\u0041\""); echo json_last_error();"#,
    );
    assert_eq!(out, "10");
}

#[test]
fn test_json_decode_high_followed_by_non_escape_sets_utf16_error() {
    let out = compile_and_run(
        r#"<?php json_decode("\"\\uD83Dx\""); echo json_last_error();"#,
    );
    assert_eq!(out, "10");
}

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

#[test]
fn test_json_decode_valid_surrogate_pair_no_error() {
    let out = compile_and_run(
        r#"<?php $x = json_decode("\"\\uD83D\\uDE00\""); echo json_last_error() . ":" . $x;"#,
    );
    // U+1F600 is "😀" (4-byte UTF-8). Verify error is 0 and the bytes round-trip.
    assert_eq!(out, "0:\u{1F600}");
}

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

#[test]
fn test_json_decode_flat_array_depth_one_fails() {
    let out = compile_and_run(
        r#"<?php json_decode("[1]", false, 1); echo json_last_error();"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_json_decode_flat_array_depth_two_succeeds() {
    let out = compile_and_run(
        r#"<?php $x = json_decode("[1]", false, 2); echo json_last_error() . ":" . count($x);"#,
    );
    assert_eq!(out, "0:1");
}

#[test]
fn test_json_decode_nested_array_depth_two_fails() {
    let out = compile_and_run(
        r#"<?php json_decode("[[1]]", false, 2); echo json_last_error();"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_json_decode_nested_array_depth_three_succeeds() {
    let out = compile_and_run(
        r#"<?php json_decode("[[1]]", false, 3); echo json_last_error();"#,
    );
    assert_eq!(out, "0");
}

#[test]
fn test_json_decode_object_depth_one_fails() {
    let out = compile_and_run(
        r#"<?php json_decode("{\"a\":1}", false, 1); echo json_last_error();"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_json_decode_scalar_depth_one_succeeds() {
    let out = compile_and_run(
        r#"<?php $x = json_decode("42", false, 1); echo json_last_error() . ":" . $x;"#,
    );
    assert_eq!(out, "0:42");
}
