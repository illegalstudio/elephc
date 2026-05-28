//! Purpose:
//! Provides json_validate syntax and depth tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - The validator must accept RFC 8259 input and reject malformed syntax without decoding.

use super::*;

/// Verifies json_validate returns a boolean type.
#[test]
fn test_json_validate_returns_bool_type() {
    let out = compile_and_run(
        "<?php $r = json_validate(\"{\\\"a\\\":1}\"); echo gettype($r);",
    );
    assert_eq!(out, "boolean");
}

/// Verifies json_validate returns true for a valid JSON object.
#[test]
fn test_json_validate_true_for_object() {
    let out = compile_and_run(
        "<?php echo (json_validate(\"{\\\"a\\\":1}\") ? \"yes\" : \"no\");",
    );
    assert_eq!(out, "yes");
}

/// Verifies json_validate accepts an optional depth argument (second param).
#[test]
fn test_json_validate_with_depth_argument() {
    let out = compile_and_run(
        "<?php echo (json_validate(\"[1,2,3]\", 16) ? \"ok\" : \"no\");",
    );
    assert_eq!(out, "ok");
}

/// Verifies json_validate accepts both depth and flags arguments.
#[test]
fn test_json_validate_with_depth_and_flags_arguments() {
    let out = compile_and_run(
        "<?php echo (json_validate(\"[1]\", 16, 0) ? \"ok\" : \"no\");",
    );
    assert_eq!(out, "ok");
}

// --- Failure paths ---

/// Verifies json_validate rejects empty input as invalid JSON.
#[test]
fn test_json_validate_rejects_empty_input() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("") ? "ok" : "no");"#,
    );
    assert_eq!(out, "no");
}

/// Verifies json_validate rejects input starting with garbage bytes.
#[test]
fn test_json_validate_rejects_garbage_first_byte() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("garbage") ? "ok" : "no");"#,
    );
    assert_eq!(out, "no");
}

/// Verifies json_validate failure sets JSON_ERROR_SYNTAX (code 4).
#[test]
fn test_json_validate_failure_sets_syntax_error_code() {
    let out = compile_and_run(
        r#"<?php json_validate("garbage"); echo json_last_error();"#,
    );
    assert_eq!(out, "4");
}

/// Verifies json_validate failure sets the error message to "Syntax error".
#[test]
fn test_json_validate_failure_sets_syntax_error_msg() {
    let out = compile_and_run(
        r#"<?php json_validate("garbage"); echo json_last_error_msg();"#,
    );
    assert_eq!(out, "Syntax error");
}

/// Verifies a successful json_validate call clears the previous error state.
#[test]
fn test_json_validate_success_clears_error_state() {
    let out = compile_and_run(
        r#"<?php json_validate("garbage"); json_validate("[1,2,3]"); echo json_last_error();"#,
    );
    assert_eq!(out, "0");
}

// --- JSON_INVALID_UTF8_IGNORE flag ---

/// Verifies JSON_INVALID_UTF8_IGNORE flag is accepted by json_validate on valid input.
#[test]
fn test_json_validate_accepts_invalid_utf8_ignore_flag_for_valid_input() {
    let out = compile_and_run(
        r#"<?php echo json_validate("[1,2,3]", 512, JSON_INVALID_UTF8_IGNORE) ? "ok" : "no";"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies JSON_INVALID_UTF8_IGNORE flag does not force a false-positive on invalid JSON.
#[test]
fn test_json_validate_invalid_utf8_ignore_does_not_throw_on_invalid_json() {
    let out = compile_and_run(
        r#"<?php echo json_validate("garbage", 512, JSON_INVALID_UTF8_IGNORE) ? "ok" : "no";"#,
    );
    assert_eq!(out, "no");
}

// --- RFC 8259 grammar coverage: scalar values ---

/// Verifies json_validate accepts all three RFC 8259 literals (null/true/false) and
/// rejects truncated/misspelled variants. Merges 5 former one-line tests into one cycle.
#[test]
fn test_json_validate_literals_all() {
    let out = compile_and_run(
        r#"<?php
echo (json_validate("null") ? "y" : "n") . "\n";
echo (json_validate("true") ? "y" : "n") . "\n";
echo (json_validate("false") ? "y" : "n") . "\n";
echo (json_validate("tru") ? "y" : "n") . "\n";
echo (json_validate("nul1") ? "y" : "n");
"#,
    );
    assert_eq!(out, "y\ny\ny\nn\nn");
}

// --- RFC 8259 number grammar ---

/// Verifies json_validate accepts a valid integer token ("0").
#[test]
fn test_json_validate_accepts_integer() {
    let out = compile_and_run(r#"<?php echo (json_validate("0") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

/// Verifies json_validate accepts a negative integer token ("-42").
#[test]
fn test_json_validate_accepts_negative_integer() {
    let out = compile_and_run(r#"<?php echo (json_validate("-42") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

/// Verifies json_validate accepts a fractional number token ("3.14").
#[test]
fn test_json_validate_accepts_fraction() {
    let out = compile_and_run(r#"<?php echo (json_validate("3.14") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

/// Verifies json_validate accepts a negative fractional token ("-0.5").
#[test]
fn test_json_validate_accepts_negative_fraction() {
    let out = compile_and_run(r#"<?php echo (json_validate("-0.5") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

/// Verifies json_validate accepts an exponent with lowercase "e" ("1e5").
#[test]
fn test_json_validate_accepts_exponent_lowercase() {
    let out = compile_and_run(r#"<?php echo (json_validate("1e5") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

/// Verifies json_validate accepts an exponent with uppercase "E" ("1E5").
#[test]
fn test_json_validate_accepts_exponent_uppercase() {
    let out = compile_and_run(r#"<?php echo (json_validate("1E5") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

/// Verifies json_validate accepts exponents with explicit sign characters ("1.5e+10", "2.5E-3").
#[test]
fn test_json_validate_accepts_exponent_with_signs() {
    let out = compile_and_run(r#"<?php echo (json_validate("1.5e+10") ? "y" : "n");"#);
    assert_eq!(out, "y");
    let out = compile_and_run(r#"<?php echo (json_validate("2.5E-3") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

/// Verifies json_validate rejects leading zero integers per RFC 8259 ("01" is invalid).
#[test]
fn test_json_validate_rejects_leading_zero_integer() {
    // RFC 8259: leading zeros are forbidden — `01` is invalid; only the
    // bare `0` (or `0` followed by `.`/`e`/`E`) is permitted.
    let out = compile_and_run(r#"<?php echo (json_validate("01") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

/// Verifies json_validate rejects a bare minus sign as an invalid number.
#[test]
fn test_json_validate_rejects_bare_minus() {
    let out = compile_and_run(r#"<?php echo (json_validate("-") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

/// Verifies json_validate rejects a trailing dot without fraction digits ("1.").
#[test]
fn test_json_validate_rejects_dot_without_fraction_digits() {
    let out = compile_and_run(r#"<?php echo (json_validate("1.") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

/// Verifies json_validate rejects numbers with exponents missing digits ("1e", "1e+").
#[test]
fn test_json_validate_rejects_exponent_without_digits() {
    let out = compile_and_run(r#"<?php echo (json_validate("1e") ? "y" : "n");"#);
    assert_eq!(out, "n");
    let out = compile_and_run(r#"<?php echo (json_validate("1e+") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

/// Verifies json_validate rejects a lone dot as an invalid number (".5" needs a leading digit).
#[test]
fn test_json_validate_rejects_dot_only() {
    let out = compile_and_run(r#"<?php echo (json_validate(".5") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

// --- RFC 8259 string grammar ---

/// Verifies json_validate accepts an empty JSON string ("").
#[test]
fn test_json_validate_accepts_empty_string() {
    let out = compile_and_run(r#"<?php echo (json_validate("\"\"") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

/// Verifies json_validate accepts a simple unquoted-free string token.
#[test]
fn test_json_validate_accepts_simple_string() {
    let out = compile_and_run(r#"<?php echo (json_validate("\"hello\"") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

/// Verifies json_validate accepts all known escape sequences (\"\\/bfnrtu).
#[test]
fn test_json_validate_accepts_known_escapes() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("\"a\\\"b\\\\c\\/d\\bf\\nf\\rf\\tx\"") ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

/// Verifies json_validate accepts a unicode escape sequence (\u00E9).
#[test]
fn test_json_validate_accepts_unicode_escape() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("\"\\u00E9\"") ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

/// Verifies json_validate rejects an unterminated string.
#[test]
fn test_json_validate_rejects_unterminated_string() {
    let out = compile_and_run(r#"<?php echo (json_validate("\"hello") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

/// Verifies json_validate rejects an unknown escape sequence (\q).
#[test]
fn test_json_validate_rejects_unknown_escape() {
    let out = compile_and_run(r#"<?php echo (json_validate("\"a\\qb\"") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

/// Verifies json_validate rejects a truncated unicode escape (\u00 with missing digits).
#[test]
fn test_json_validate_rejects_truncated_unicode_escape() {
    let out = compile_and_run(r#"<?php echo (json_validate("\"\\u00\"") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

/// Verifies json_validate rejects a unicode escape with non-hex digits (\u00ZZ).
#[test]
fn test_json_validate_rejects_non_hex_unicode_digit() {
    let out = compile_and_run(r#"<?php echo (json_validate("\"\\u00ZZ\"") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

/// Verifies json_validate rejects an unescaped control character (chr(0x01) < 0x20).
#[test]
fn test_json_validate_rejects_unescaped_control_char() {
    // chr(0x01) is below 0x20 and must be rejected unescaped inside strings.
    let out = compile_and_run(
        r#"<?php echo (json_validate("\"a" . chr(0x01) . "b\"") ? "y" : "n");"#,
    );
    assert_eq!(out, "n");
}

// --- RFC 8259 array grammar ---

/// Verifies json_validate accepts an empty JSON array ([]).
#[test]
fn test_json_validate_accepts_empty_array() {
    let out = compile_and_run(r#"<?php echo (json_validate("[]") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

/// Verifies json_validate accepts a simple array with integer elements.
#[test]
fn test_json_validate_accepts_simple_array() {
    let out = compile_and_run(r#"<?php echo (json_validate("[1,2,3]") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

/// Verifies json_validate accepts an array containing mixed JSON value types.
#[test]
fn test_json_validate_accepts_array_with_mixed_types() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("[1,\"two\",true,null,3.14]") ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

/// Verifies json_validate accepts nested arrays including empty sub-arrays.
#[test]
fn test_json_validate_accepts_nested_array() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("[[1,2],[3,4],[]]") ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

/// Verifies json_validate accepts arrays with whitespace around elements.
#[test]
fn test_json_validate_accepts_array_with_whitespace() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("[ 1 , 2 , 3 ]") ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

/// Verifies json_validate rejects a trailing comma inside an array ([1,2,]).
#[test]
fn test_json_validate_rejects_array_trailing_comma() {
    let out = compile_and_run(r#"<?php echo (json_validate("[1,2,]") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

/// Verifies json_validate rejects an array missing a comma between elements.
#[test]
fn test_json_validate_rejects_array_missing_comma() {
    let out = compile_and_run(r#"<?php echo (json_validate("[1 2]") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

/// Verifies json_validate rejects an unclosed array bracket.
#[test]
fn test_json_validate_rejects_unterminated_array() {
    let out = compile_and_run(r#"<?php echo (json_validate("[1,2") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

/// Verifies json_validate rejects an array with an extra closing bracket.
#[test]
fn test_json_validate_rejects_array_extra_close() {
    let out = compile_and_run(r#"<?php echo (json_validate("[1,2]]") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

// --- RFC 8259 object grammar ---

/// Verifies json_validate accepts an empty JSON object ({}).
#[test]
fn test_json_validate_accepts_empty_object() {
    let out = compile_and_run(r#"<?php echo (json_validate("{}") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

/// Verifies json_validate accepts a simple object with string keys and numeric values.
#[test]
fn test_json_validate_accepts_simple_object() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("{\"a\":1,\"b\":2}") ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

/// Verifies json_validate accepts nested objects.
#[test]
fn test_json_validate_accepts_nested_object() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("{\"x\":{\"y\":{\"z\":1}}}") ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

/// Verifies json_validate accepts objects with whitespace around keys and values.
#[test]
fn test_json_validate_accepts_object_with_whitespace() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("{ \"a\" : 1 , \"b\" : 2 }") ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

/// Verifies json_validate rejects a trailing comma inside an object ({"a":1,}).
#[test]
fn test_json_validate_rejects_object_trailing_comma() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("{\"a\":1,}") ? "y" : "n");"#,
    );
    assert_eq!(out, "n");
}

/// Verifies json_validate rejects an object missing a colon between key and value.
#[test]
fn test_json_validate_rejects_object_missing_colon() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("{\"a\" 1}") ? "y" : "n");"#,
    );
    assert_eq!(out, "n");
}

/// Verifies json_validate rejects an object with an unquoted bare key ({a:1}).
#[test]
fn test_json_validate_rejects_object_bare_key() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("{a:1}") ? "y" : "n");"#,
    );
    assert_eq!(out, "n");
}

/// Verifies json_validate rejects an unclosed object bracket.
#[test]
fn test_json_validate_rejects_unterminated_object() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("{\"a\":1") ? "y" : "n");"#,
    );
    assert_eq!(out, "n");
}

// --- Trailing junk and whitespace policy ---

/// Verifies json_validate rejects input with trailing junk after the JSON value.
#[test]
fn test_json_validate_rejects_trailing_junk() {
    let out = compile_and_run(r#"<?php echo (json_validate("[1] junk") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

/// Verifies json_validate accepts leading and trailing whitespace around valid JSON.
/// Uses chr(13) for real 0x0D since the lexer does not parse \r directly.
#[test]
fn test_json_validate_accepts_leading_and_trailing_whitespace() {
    // elephc's lexer only translates the \n, \t, \\, \", \$, \0 escapes;
    // \r is left as a literal backslash-r pair. Build the CR byte via chr()
    // so the validator sees a real 0x0D byte.
    let out = compile_and_run(
        r#"<?php echo (json_validate("   \n\t [1,2,3] " . chr(13) . "\n  ") ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

/// Verifies json_validate rejects two concatenated JSON values (no separator).
#[test]
fn test_json_validate_rejects_two_concatenated_values() {
    let out = compile_and_run(r#"<?php echo (json_validate("1 2") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

// --- Depth enforcement ---

/// Verifies json_validate rejects nesting that exceeds the depth limit (6 levels, limit=3).
#[test]
fn test_json_validate_rejects_depth_overflow() {
    // 6 levels of nesting against a depth limit of 3 → JSON_ERROR_DEPTH.
    let out = compile_and_run(
        r#"<?php echo (json_validate("[[[[[[1]]]]]]", 3) ? "y" : "n");"#,
    );
    assert_eq!(out, "n");
}

/// Verifies depth overflow sets JSON_ERROR_DEPTH (code 1).
#[test]
fn test_json_validate_depth_overflow_sets_depth_error() {
    let out = compile_and_run(
        r#"<?php
json_validate("[[[[[[1]]]]]]", 3);
echo json_last_error();
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies depth overflow sets JSON_ERROR_DEPTH even when JSON_INVALID_UTF8_IGNORE flag is set.
#[test]
fn test_json_validate_depth_overflow_with_allowed_flag_sets_depth_error() {
    let out = compile_and_run(
        r#"<?php
json_validate("[[[[[[1]]]]]]", 3, JSON_INVALID_UTF8_IGNORE);
echo json_last_error();
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies json_validate rejects when active nesting depth equals the limit (strict semantics).
/// 5 nested containers with depth=5 → fails because active == limit is rejected.
#[test]
fn test_json_validate_rejects_nesting_at_depth_limit() {
    // PHP json_validate rejects when the active nesting depth equals the
    // limit (strict semantics). A depth of 5 admits at most 4 nested
    // containers; `[[[[[1]]]]]` has 5 levels and therefore fails.
    let out = compile_and_run(
        r#"<?php echo (json_validate("[[[[[1]]]]]", 5) ? "y" : "n");"#,
    );
    assert_eq!(out, "n");
}

/// Verifies json_validate accepts nesting when active depth is below the limit (5 < 6).
#[test]
fn test_json_validate_accepts_nesting_below_depth_limit() {
    // 5 nested containers fit when depth=6 (active=5 < limit=6).
    let out = compile_and_run(
        r#"<?php echo (json_validate("[[[[[1]]]]]", 6) ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

// --- Mixed validity examples ---

/// Verifies json_validate accepts a realistic multi-structure JSON payload.
#[test]
fn test_json_validate_accepts_realistic_payload() {
    let out = compile_and_run(
        r#"<?php
$payload = "{\"users\":[{\"name\":\"Alice\",\"age\":30},{\"name\":\"Bob\",\"age\":25}],\"count\":2,\"ok\":true}";
echo (json_validate($payload) ? "y" : "n");
"#,
    );
    assert_eq!(out, "y");
}

/// Verifies json_validate catches a missing comma between object elements that bracket
/// pairing alone would not detect.
#[test]
fn test_json_validate_rejects_subtly_malformed_payload() {
    // Missing comma between the two array elements — bracket pairing alone
    // would not catch this; the recursive validator does.
    let out = compile_and_run(
        r#"<?php
$bad = "{\"users\":[{\"name\":\"Alice\"} {\"name\":\"Bob\"}]}";
echo (json_validate($bad) ? "y" : "n");
"#,
    );
    assert_eq!(out, "n");
}

/// Verifies json_validate returns false and sets JSON_ERROR_UTF16 (code 10) for
/// a lone high surrogate (\uD83D) without a following low surrogate.
#[test]
fn test_json_validate_lone_high_surrogate_returns_false() {
    let out = compile_and_run(
        r#"<?php echo json_validate("\"\\uD83D\"") ? "ok" : ("no:" . json_last_error());"#,
    );
    assert_eq!(out, "no:10");
}

/// Verifies json_validate returns false and sets JSON_ERROR_UTF16 (code 10) for
/// a lone low surrogate (\uDE00) without a preceding high surrogate.
#[test]
fn test_json_validate_lone_low_surrogate_returns_false() {
    let out = compile_and_run(
        r#"<?php echo json_validate("\"\\uDE00\"") ? "ok" : ("no:" . json_last_error());"#,
    );
    assert_eq!(out, "no:10");
}

/// Verifies json_validate returns true for a valid surrogate pair (\uD83D\uDE00 = 😀).
#[test]
fn test_json_validate_valid_surrogate_pair_returns_true() {
    let out = compile_and_run(
        r#"<?php echo json_validate("\"\\uD83D\\uDE00\"") ? "ok" : "no";"#,
    );
    assert_eq!(out, "ok");
}
