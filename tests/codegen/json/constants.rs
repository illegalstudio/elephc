//! Purpose:
//! Provides JSON constant codegen tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Constant values must match PHP JSON_ERROR_* and JSON flag bitmasks.

use super::*;

/// Verifies every JSON_* integer constant exposed by the runtime resolves to the PHP-spec
/// value. Merges 12 former one-line tests into one compile/link/run cycle.
#[test]
fn test_json_all_int_constants() {
    let out = compile_and_run(
        r#"<?php
echo JSON_PRETTY_PRINT . "\n";
echo JSON_UNESCAPED_SLASHES . "\n";
echo JSON_THROW_ON_ERROR . "\n";
echo JSON_FORCE_OBJECT . "\n";
echo JSON_UNESCAPED_UNICODE . "\n";
echo JSON_OBJECT_AS_ARRAY . "\n";
echo JSON_BIGINT_AS_STRING . "\n";
echo JSON_INVALID_UTF8_IGNORE . "\n";
echo JSON_INVALID_UTF8_SUBSTITUTE . "\n";
echo JSON_PARTIAL_OUTPUT_ON_ERROR . "\n";
echo JSON_PRESERVE_ZERO_FRACTION . "\n";
echo JSON_NUMERIC_CHECK . "\n";
echo JSON_ERROR_NONE;
"#,
    );
    assert_eq!(
        out,
        "128\n64\n4194304\n16\n256\n1\n2\n1048576\n2097152\n512\n1024\n32\n0",
    );
}

/// Verifies the JSON_HEX_TAG / JSON_HEX_AMP / JSON_HEX_APOS / JSON_HEX_QUOT hex-family
/// constants resolve to the expected bitmask values.
#[test]
fn test_json_hex_family_constants() {
    let out = compile_and_run(
        r#"<?php echo JSON_HEX_TAG . "," . JSON_HEX_AMP . "," . JSON_HEX_APOS . "," . JSON_HEX_QUOT;"#,
    );
    assert_eq!(out, "1,2,4,8");
}

/// Verifies all JSON_ERROR_* depth/state/UTF constants resolve to their PHP code values.
/// Merges 10 former one-line tests into one compile/link/run cycle.
#[test]
fn test_json_error_codes_sequence() {
    let out = compile_and_run(
        r#"<?php
echo JSON_ERROR_DEPTH . ",";
echo JSON_ERROR_STATE_MISMATCH . ",";
echo JSON_ERROR_CTRL_CHAR . ",";
echo JSON_ERROR_SYNTAX . ",";
echo JSON_ERROR_UTF8 . ",";
echo JSON_ERROR_RECURSION . ",";
echo JSON_ERROR_INF_OR_NAN . ",";
echo JSON_ERROR_UNSUPPORTED_TYPE . ",";
echo JSON_ERROR_INVALID_PROPERTY_NAME . ",";
echo JSON_ERROR_UTF16;
"#,
    );
    assert_eq!(out, "1,2,3,4,5,6,7,8,9,10");
}

/// Verifies JSON constant values combine with bitwise OR to produce the expected sum.
#[test]
fn test_json_constants_compose_with_or() {
    let out = compile_and_run(
        "<?php echo (JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);",
    );
    assert_eq!(out, "448");
}

/// Verifies JSON constants resolve to their integer values when used as function call
/// arguments, even though json_encode currently ignores flags.
#[test]
fn test_json_constants_in_function_call_argument() {
    // Even though json_encode currently ignores flags, the constant must
    // resolve to its int value when passed as an argument.
    let out = compile_and_run(
        "<?php $f = JSON_PRETTY_PRINT; echo $f + JSON_UNESCAPED_SLASHES;",
    );
    assert_eq!(out, "192");
}
