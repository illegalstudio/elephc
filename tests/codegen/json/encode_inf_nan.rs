//! Purpose:
//! Provides JSON encode non-finite-float tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - INF/NAN handling must coordinate false returns, partial output, and JsonException.

use super::*;

// __rt_json_encode_float intercepts Inf/NaN before __rt_ftoa, sets
// JSON_ERROR_INF_OR_NAN, throws when JSON_THROW_ON_ERROR is set, and
// otherwise lets the wrapper return false unless JSON_PARTIAL_OUTPUT_ON_ERROR
// asks for the substituted partial JSON.

/// Verifies a finite float (3.14) encodes unchanged (starts_with due to float formatting precision).
#[test]
fn test_json_encode_finite_float_unchanged() {
    let out = compile_and_run("<?php echo json_encode(3.14);");
    assert!(out.starts_with("3.14"), "Got: {}", out);
}

/// Verifies json_encode(INF) without flags echoes false as empty string (the boolean false
/// cast to string produces "").
#[test]
fn test_json_encode_inf_without_flag_echoes_false_as_empty() {
    let out = compile_and_run("<?php echo json_encode(INF);");
    assert_eq!(out, "");
}

/// Verifies json_encode(INF) returns strict false (=== false), not a falsy string.
#[test]
fn test_json_encode_inf_without_flag_is_strict_false() {
    let out = compile_and_run("<?php echo json_encode(INF) === false ? 'false' : 'json';");
    assert_eq!(out, "false");
}

/// Verifies json_encode(INF) without flags sets JSON_ERROR_INF_OR_NAN (7).
#[test]
fn test_json_encode_inf_without_flag_sets_error_code() {
    let out = compile_and_run(
        "<?php json_encode(INF); echo json_last_error();",
    );
    assert_eq!(out, "7");
}

/// Verifies json_encode(INF) without flags sets the error message to
/// "Inf and NaN cannot be JSON encoded".
#[test]
fn test_json_encode_inf_without_flag_sets_error_msg() {
    let out = compile_and_run(
        "<?php json_encode(INF); echo json_last_error_msg();",
    );
    assert_eq!(out, "Inf and NaN cannot be JSON encoded");
}

/// Verifies json_encode(NAN) without flags echoes false as empty string.
#[test]
fn test_json_encode_nan_without_flag_echoes_false_as_empty() {
    let out = compile_and_run("<?php echo json_encode(NAN);");
    assert_eq!(out, "");
}

/// Verifies json_encode(NAN) returns strict false (=== false).
#[test]
fn test_json_encode_nan_without_flag_is_strict_false() {
    let out = compile_and_run("<?php echo json_encode(NAN) === false ? 'false' : 'json';");
    assert_eq!(out, "false");
}

/// Verifies json_encode(NAN) without flags sets JSON_ERROR_INF_OR_NAN (7).
#[test]
fn test_json_encode_nan_without_flag_sets_error_code() {
    let out = compile_and_run(
        "<?php json_encode(NAN); echo json_last_error();",
    );
    assert_eq!(out, "7");
}

/// Verifies JSON_THROW_ON_ERROR raises JsonException with "Inf and NaN cannot be JSON encoded"
/// when encoding INF.
#[test]
fn test_json_encode_inf_throws_when_flag_set() {
    let out = compile_and_run(
        r#"<?php
try {
    json_encode(INF, JSON_THROW_ON_ERROR);
    echo "no throw";
} catch (JsonException $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(out, "Inf and NaN cannot be JSON encoded");
}

/// Verifies JSON_THROW_ON_ERROR raises JsonException with "Inf and NaN cannot be JSON encoded"
/// when encoding NAN.
#[test]
fn test_json_encode_nan_throws_when_flag_set() {
    let out = compile_and_run(
        r#"<?php
try {
    json_encode(NAN, JSON_THROW_ON_ERROR);
    echo "no throw";
} catch (JsonException $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(out, "Inf and NaN cannot be JSON encoded");
}

/// Verifies INF inside an array element throws JsonException via __rt_json_encode_float dispatch.
#[test]
fn test_json_encode_inf_inside_array_throws_when_flag_set() {
    // Array element float dispatch routes through __rt_json_encode_float too.
    let out = compile_and_run(
        r#"<?php
try {
    json_encode([1.5, INF, 2.5], JSON_THROW_ON_ERROR);
    echo "no throw";
} catch (JsonException $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(out, "Inf and NaN cannot be JSON encoded");
}

/// Verifies JsonException from INF encoding is catchable as RuntimeException (parent class).
#[test]
fn test_json_encode_inf_caught_as_runtime_exception() {
    let out = compile_and_run(
        r#"<?php
try { json_encode(INF, JSON_THROW_ON_ERROR); }
catch (RuntimeException $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "Inf and NaN cannot be JSON encoded");
}

/// Verifies negative INF is also detected and rejected without flags (echoes false as empty).
#[test]
fn test_json_encode_negative_inf_also_detected() {
    let out = compile_and_run("<?php echo json_encode(-INF);");
    assert_eq!(out, "");
}

/// Verifies JSON_PARTIAL_OUTPUT_ON_ERROR substitutes INF with 0 in the output JSON string.
/// PHP replaces the non-finite value with null in the partial output.
#[test]
fn test_json_encode_partial_output_flag_keeps_substituted_float_json() {
    let out = compile_and_run("<?php echo json_encode(INF, JSON_PARTIAL_OUTPUT_ON_ERROR);");
    assert_eq!(out, "0");
}

/// Verifies an array containing INF without JSON_PARTIAL_OUTPUT_ON_ERROR returns false.
#[test]
fn test_json_encode_array_with_inf_without_partial_flag_is_false() {
    let out = compile_and_run("<?php echo json_encode([1.5, INF, 2.5]) === false ? 'false' : 'json';");
    assert_eq!(out, "false");
}

/// Verifies JSON_PARTIAL_OUTPUT_ON_ERROR lets an array with INF produce substituted JSON "[1.5,0,2.5]".
#[test]
fn test_json_encode_array_with_inf_partial_output_keeps_json() {
    let out = compile_and_run(
        "<?php echo json_encode([1.5, INF, 2.5], JSON_PARTIAL_OUTPUT_ON_ERROR);",
    );
    assert_eq!(out, "[1.5,0,2.5]");
}

/// Verifies a successful json_encode call clears the previous JSON_ERROR_INF_OR_NAN;
/// the second call sees JSON_ERROR_NONE even though the first reported INF_OR_NAN.
/// This matches PHP's per-call error reset behavior.
#[test]
fn test_json_encode_finite_clears_previous_error() {
    let out = compile_and_run(
        r#"<?php
json_encode(INF);
$first = json_last_error();
json_encode(3.14);
echo $first . "/" . json_last_error();
"#,
    );
    // The json_encode wrapper resets _json_last_error at the start of every
    // call (matching PHP), so the second invocation sees JSON_ERROR_NONE
    // even though the first reported INF_OR_NAN.
    assert_eq!(out, "7/0");
}
