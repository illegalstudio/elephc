//! Purpose:
//! Provides JSON_BIGINT_AS_STRING decode tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Overflow detection must preserve integer token spelling when requested.

use super::*;

// JSON_BIGINT_AS_STRING decode-flag tests.
//
// Without the flag, integer-grammar JSON tokens that overflow PHP_INT
// (i64) are returned as floats — the existing __rt_atoi path silently
// wraps and would produce garbage, so the runtime must promote those
// tokens to a Mixed(string) when the flag is set, preserving the
// original digits. Float-grammar tokens (with `.`, `e`, or `E`) and
// in-range integers are unaffected by the flag.

/// Verifies JSON integers exceeding PHP_INT_MAX become `double` when JSON_BIGINT_AS_STRING is absent.
/// Without the flag the runtime cannot represent overflowing integers, so silent promotion to float is the only safe option.
#[test]
fn bigint_without_flag_becomes_float() {
    let out = compile_and_run(
        r#"<?php $x = json_decode("999999999999999999999"); echo gettype($x);"#,
    );
    assert_eq!(out, "double");
}

/// Verifies JSON_BIGINT_AS_STRING causes overflow-length positive integers to be returned as string,
/// preserving the original digit sequence exactly as it appeared in the JSON source.
#[test]
fn bigint_with_flag_becomes_string() {
    let out = compile_and_run(
        r#"<?php $x = json_decode("999999999999999999999", false, 512, JSON_BIGINT_AS_STRING); echo gettype($x) . ":" . $x;"#,
    );
    assert_eq!(out, "string:999999999999999999999");
}

/// Verifies JSON_BIGINT_AS_STRING causes overflow-length negative integers to be returned as string.
/// The leading minus is part of the token spelling and must be preserved.
#[test]
fn negative_bigint_with_flag_becomes_string() {
    let out = compile_and_run(
        r#"<?php $x = json_decode("-9223372036854775809", false, 512, JSON_BIGINT_AS_STRING); echo gettype($x) . ":" . $x;"#,
    );
    assert_eq!(out, "string:-9223372036854775809");
}

/// Verifies PHP_INT_MAX itself is NOT promoted to string — the flag applies only to overflow tokens.
/// This is a regression guard: in-range integers must remain integer.
#[test]
fn int_max_with_flag_stays_int() {
    // PHP_INT_MAX itself fits — the flag must NOT promote it to string.
    let out = compile_and_run(
        r#"<?php $x = json_decode("9223372036854775807", false, 512, JSON_BIGINT_AS_STRING); echo gettype($x) . ":" . $x;"#,
    );
    assert_eq!(out, "integer:9223372036854775807");
}

/// Verifies PHP_INT_MAX + 1 becomes string even though it shares the same digit length as PHP_INT_MAX.
/// This is a regression guard for length-then-lex overflow detection at the final digit.
#[test]
fn int_max_plus_one_with_flag_becomes_string() {
    // Just past PHP_INT_MAX — same length as the threshold, so the
    // length-then-lex compare must catch the overflow at the last digit.
    let out = compile_and_run(
        r#"<?php $x = json_decode("9223372036854775808", false, 512, JSON_BIGINT_AS_STRING); echo $x;"#,
    );
    assert_eq!(out, "9223372036854775808");
}

/// Verifies PHP_INT_MIN itself is NOT promoted to string — the flag applies only to overflow tokens.
/// This is a regression guard: in-range negative integers must remain integer.
#[test]
fn int_min_with_flag_stays_int() {
    // PHP_INT_MIN itself fits exactly — must NOT promote.
    let out = compile_and_run(
        r#"<?php $x = json_decode("-9223372036854775808", false, 512, JSON_BIGINT_AS_STRING); echo gettype($x);"#,
    );
    assert_eq!(out, "integer");
}

/// Verifies the flag is ignored for JSON numbers containing a decimal point — float grammar takes precedence.
#[test]
fn huge_float_with_flag_stays_float() {
    // The `.` triggers the float grammar — flag must be ignored.
    let out = compile_and_run(
        r#"<?php $x = json_decode("9999999999999999999.5", false, 512, JSON_BIGINT_AS_STRING); echo gettype($x);"#,
    );
    assert_eq!(out, "double");
}

/// Verifies the flag is ignored for JSON numbers containing an exponent marker (`e` or `E`).
#[test]
fn exponent_with_flag_stays_float() {
    // The `e` triggers the float grammar — flag must be ignored.
    let out = compile_and_run(
        r#"<?php $x = json_decode("1e25", false, 512, JSON_BIGINT_AS_STRING); echo gettype($x);"#,
    );
    assert_eq!(out, "double");
}

/// Verifies the flag has no effect on in-range integers — trivial case, result must be integer.
#[test]
fn small_int_with_flag_stays_int() {
    // Trivial case — small integer well within range, flag does nothing.
    let out = compile_and_run(
        r#"<?php $x = json_decode("42", false, 512, JSON_BIGINT_AS_STRING); echo gettype($x) . ":" . $x;"#,
    );
    assert_eq!(out, "integer:42");
}

/// Verifies _json_active_flags is consulted on every recursive number decode, not just at the top level.
/// Without recursive flag propagation nested overflow integers would incorrectly become float.
#[test]
fn bigint_in_array_with_flag_becomes_string() {
    // Verifies _json_active_flags is read on every recursive number decode,
    // not just at the top level.
    let out = compile_and_run(
        r#"<?php $a = json_decode("[10000000000000000000]", false, 512, JSON_BIGINT_AS_STRING); echo gettype($a[0]) . ":" . $a[0];"#,
    );
    assert_eq!(out, "string:10000000000000000000");
}
