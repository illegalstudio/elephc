use super::*;

#[test]
fn test_json_validate_returns_bool_type() {
    let out = compile_and_run(
        "<?php $r = json_validate(\"{\\\"a\\\":1}\"); echo gettype($r);",
    );
    assert_eq!(out, "boolean");
}

#[test]
fn test_json_validate_true_for_object() {
    // Phase 2 stub: returns true for any input that the existing decoder
    // accepts. Phase 6 will tighten this with real syntax checking.
    let out = compile_and_run(
        "<?php echo (json_validate(\"{\\\"a\\\":1}\") ? \"yes\" : \"no\");",
    );
    assert_eq!(out, "yes");
}

#[test]
fn test_json_validate_with_depth_argument() {
    let out = compile_and_run(
        "<?php echo (json_validate(\"[1,2,3]\", 16) ? \"ok\" : \"no\");",
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_json_validate_with_depth_and_flags_arguments() {
    let out = compile_and_run(
        "<?php echo (json_validate(\"[1]\", 16, 0) ? \"ok\" : \"no\");",
    );
    assert_eq!(out, "ok");
}

// --- Failure paths ---

#[test]
fn test_json_validate_rejects_empty_input() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("") ? "ok" : "no");"#,
    );
    assert_eq!(out, "no");
}

#[test]
fn test_json_validate_rejects_garbage_first_byte() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("garbage") ? "ok" : "no");"#,
    );
    assert_eq!(out, "no");
}

#[test]
fn test_json_validate_failure_sets_syntax_error_code() {
    let out = compile_and_run(
        r#"<?php json_validate("garbage"); echo json_last_error();"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_json_validate_failure_sets_syntax_error_msg() {
    let out = compile_and_run(
        r#"<?php json_validate("garbage"); echo json_last_error_msg();"#,
    );
    assert_eq!(out, "Syntax error");
}

#[test]
fn test_json_validate_success_clears_error_state() {
    let out = compile_and_run(
        r#"<?php json_validate("garbage"); json_validate("[1,2,3]"); echo json_last_error();"#,
    );
    assert_eq!(out, "0");
}

// --- JSON_INVALID_UTF8_IGNORE flag ---

#[test]
fn test_json_validate_accepts_invalid_utf8_ignore_flag_for_valid_input() {
    let out = compile_and_run(
        r#"<?php echo json_validate("[1,2,3]", 512, JSON_INVALID_UTF8_IGNORE) ? "ok" : "no";"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_json_validate_invalid_utf8_ignore_does_not_throw_on_invalid_json() {
    let out = compile_and_run(
        r#"<?php echo json_validate("garbage", 512, JSON_INVALID_UTF8_IGNORE) ? "ok" : "no";"#,
    );
    assert_eq!(out, "no");
}

// --- RFC 8259 grammar coverage: scalar values ---

// Verify json_validate's RFC 8259 literal recognition: the three accepted
// literals (`null` / `true` / `false`) plus a truncated and a misspelled
// rejection. Originally these were 5 separate tests; we merge them into a
// single multi-echo cycle.
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

#[test]
fn test_json_validate_accepts_integer() {
    let out = compile_and_run(r#"<?php echo (json_validate("0") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_accepts_negative_integer() {
    let out = compile_and_run(r#"<?php echo (json_validate("-42") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_accepts_fraction() {
    let out = compile_and_run(r#"<?php echo (json_validate("3.14") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_accepts_negative_fraction() {
    let out = compile_and_run(r#"<?php echo (json_validate("-0.5") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_accepts_exponent_lowercase() {
    let out = compile_and_run(r#"<?php echo (json_validate("1e5") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_accepts_exponent_uppercase() {
    let out = compile_and_run(r#"<?php echo (json_validate("1E5") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_accepts_exponent_with_signs() {
    let out = compile_and_run(r#"<?php echo (json_validate("1.5e+10") ? "y" : "n");"#);
    assert_eq!(out, "y");
    let out = compile_and_run(r#"<?php echo (json_validate("2.5E-3") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_rejects_leading_zero_integer() {
    // RFC 8259: leading zeros are forbidden — `01` is invalid; only the
    // bare `0` (or `0` followed by `.`/`e`/`E`) is permitted.
    let out = compile_and_run(r#"<?php echo (json_validate("01") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

#[test]
fn test_json_validate_rejects_bare_minus() {
    let out = compile_and_run(r#"<?php echo (json_validate("-") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

#[test]
fn test_json_validate_rejects_dot_without_fraction_digits() {
    let out = compile_and_run(r#"<?php echo (json_validate("1.") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

#[test]
fn test_json_validate_rejects_exponent_without_digits() {
    let out = compile_and_run(r#"<?php echo (json_validate("1e") ? "y" : "n");"#);
    assert_eq!(out, "n");
    let out = compile_and_run(r#"<?php echo (json_validate("1e+") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

#[test]
fn test_json_validate_rejects_dot_only() {
    let out = compile_and_run(r#"<?php echo (json_validate(".5") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

// --- RFC 8259 string grammar ---

#[test]
fn test_json_validate_accepts_empty_string() {
    let out = compile_and_run(r#"<?php echo (json_validate("\"\"") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_accepts_simple_string() {
    let out = compile_and_run(r#"<?php echo (json_validate("\"hello\"") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_accepts_known_escapes() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("\"a\\\"b\\\\c\\/d\\bf\\nf\\rf\\tx\"") ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_accepts_unicode_escape() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("\"\\u00E9\"") ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_rejects_unterminated_string() {
    let out = compile_and_run(r#"<?php echo (json_validate("\"hello") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

#[test]
fn test_json_validate_rejects_unknown_escape() {
    let out = compile_and_run(r#"<?php echo (json_validate("\"a\\qb\"") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

#[test]
fn test_json_validate_rejects_truncated_unicode_escape() {
    let out = compile_and_run(r#"<?php echo (json_validate("\"\\u00\"") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

#[test]
fn test_json_validate_rejects_non_hex_unicode_digit() {
    let out = compile_and_run(r#"<?php echo (json_validate("\"\\u00ZZ\"") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

#[test]
fn test_json_validate_rejects_unescaped_control_char() {
    // chr(0x01) is below 0x20 and must be rejected unescaped inside strings.
    let out = compile_and_run(
        r#"<?php echo (json_validate("\"a" . chr(0x01) . "b\"") ? "y" : "n");"#,
    );
    assert_eq!(out, "n");
}

// --- RFC 8259 array grammar ---

#[test]
fn test_json_validate_accepts_empty_array() {
    let out = compile_and_run(r#"<?php echo (json_validate("[]") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_accepts_simple_array() {
    let out = compile_and_run(r#"<?php echo (json_validate("[1,2,3]") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_accepts_array_with_mixed_types() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("[1,\"two\",true,null,3.14]") ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_accepts_nested_array() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("[[1,2],[3,4],[]]") ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_accepts_array_with_whitespace() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("[ 1 , 2 , 3 ]") ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_rejects_array_trailing_comma() {
    let out = compile_and_run(r#"<?php echo (json_validate("[1,2,]") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

#[test]
fn test_json_validate_rejects_array_missing_comma() {
    let out = compile_and_run(r#"<?php echo (json_validate("[1 2]") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

#[test]
fn test_json_validate_rejects_unterminated_array() {
    let out = compile_and_run(r#"<?php echo (json_validate("[1,2") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

#[test]
fn test_json_validate_rejects_array_extra_close() {
    let out = compile_and_run(r#"<?php echo (json_validate("[1,2]]") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

// --- RFC 8259 object grammar ---

#[test]
fn test_json_validate_accepts_empty_object() {
    let out = compile_and_run(r#"<?php echo (json_validate("{}") ? "y" : "n");"#);
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_accepts_simple_object() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("{\"a\":1,\"b\":2}") ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_accepts_nested_object() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("{\"x\":{\"y\":{\"z\":1}}}") ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_accepts_object_with_whitespace() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("{ \"a\" : 1 , \"b\" : 2 }") ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

#[test]
fn test_json_validate_rejects_object_trailing_comma() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("{\"a\":1,}") ? "y" : "n");"#,
    );
    assert_eq!(out, "n");
}

#[test]
fn test_json_validate_rejects_object_missing_colon() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("{\"a\" 1}") ? "y" : "n");"#,
    );
    assert_eq!(out, "n");
}

#[test]
fn test_json_validate_rejects_object_bare_key() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("{a:1}") ? "y" : "n");"#,
    );
    assert_eq!(out, "n");
}

#[test]
fn test_json_validate_rejects_unterminated_object() {
    let out = compile_and_run(
        r#"<?php echo (json_validate("{\"a\":1") ? "y" : "n");"#,
    );
    assert_eq!(out, "n");
}

// --- Trailing junk and whitespace policy ---

#[test]
fn test_json_validate_rejects_trailing_junk() {
    let out = compile_and_run(r#"<?php echo (json_validate("[1] junk") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

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

#[test]
fn test_json_validate_rejects_two_concatenated_values() {
    let out = compile_and_run(r#"<?php echo (json_validate("1 2") ? "y" : "n");"#);
    assert_eq!(out, "n");
}

// --- Depth enforcement ---

#[test]
fn test_json_validate_rejects_depth_overflow() {
    // 6 levels of nesting against a depth limit of 3 → JSON_ERROR_DEPTH.
    let out = compile_and_run(
        r#"<?php echo (json_validate("[[[[[[1]]]]]]", 3) ? "y" : "n");"#,
    );
    assert_eq!(out, "n");
}

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

#[test]
fn test_json_validate_accepts_nesting_below_depth_limit() {
    // 5 nested containers fit when depth=6 (active=5 < limit=6).
    let out = compile_and_run(
        r#"<?php echo (json_validate("[[[[[1]]]]]", 6) ? "y" : "n");"#,
    );
    assert_eq!(out, "y");
}

// --- Mixed validity examples ---

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

#[test]
fn test_json_validate_lone_high_surrogate_returns_false() {
    let out = compile_and_run(
        r#"<?php echo json_validate("\"\\uD83D\"") ? "ok" : ("no:" . json_last_error());"#,
    );
    assert_eq!(out, "no:10");
}

#[test]
fn test_json_validate_lone_low_surrogate_returns_false() {
    let out = compile_and_run(
        r#"<?php echo json_validate("\"\\uDE00\"") ? "ok" : ("no:" . json_last_error());"#,
    );
    assert_eq!(out, "no:10");
}

#[test]
fn test_json_validate_valid_surrogate_pair_returns_true() {
    let out = compile_and_run(
        r#"<?php echo json_validate("\"\\uD83D\\uDE00\"") ? "ok" : "no";"#,
    );
    assert_eq!(out, "ok");
}
