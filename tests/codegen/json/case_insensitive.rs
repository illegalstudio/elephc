//! Purpose:
//! Provides JSON builtin namespace and case-insensitivity codegen tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - PHP-visible builtin lookup must work through mixed case and namespace fallback.

use super::*;

// PHP-visible builtins must be reachable through case-insensitive and
// namespaced call syntax (CLAUDE.md mandate). These tests cover the
// JSON public surface to lock the contract in place.

/// Verifies json_encode is reachable using mixed-case spelling (Json_Encode).
#[test]
fn json_encode_case_insensitive() {
    let out = compile_and_run(r#"<?php echo Json_Encode([1, 2]);"#);
    assert_eq!(out, "[1,2]");
}

/// Verifies json_encode is reachable using all-caps spelling (JSON_ENCODE).
#[test]
fn json_encode_uppercase() {
    let out = compile_and_run(r#"<?php echo JSON_ENCODE("hi");"#);
    assert_eq!(out, "\"hi\"");
}

/// Verifies json_encode is reachable via the root namespace escape (\json_encode).
#[test]
fn json_encode_namespaced() {
    let out = compile_and_run(r#"<?php echo \json_encode(true);"#);
    assert_eq!(out, "true");
}

/// Verifies json_decode is reachable using mixed-case spelling (JSON_DECODE).
#[test]
fn json_decode_case_insensitive() {
    let out = compile_and_run(r#"<?php echo JSON_DECODE("\"hi\"");"#);
    assert_eq!(out, "hi");
}

/// Verifies json_decode is reachable via the root namespace escape (\json_decode).
#[test]
fn json_decode_namespaced() {
    let out = compile_and_run(r#"<?php echo \json_decode("42");"#);
    assert_eq!(out, "42");
}

/// Verifies json_validate is reachable using mixed-case spelling (Json_Validate).
#[test]
fn json_validate_case_insensitive() {
    let out = compile_and_run(
        r#"<?php echo Json_Validate("[1,2]") ? "ok" : "no";"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies json_validate is reachable via the root namespace escape (\json_validate).
#[test]
fn json_validate_namespaced() {
    let out = compile_and_run(
        r#"<?php echo \json_validate("not json") ? "ok" : "no";"#,
    );
    assert_eq!(out, "no");
}

/// Verifies json_last_error is reachable using mixed-case spelling (Json_Last_Error).
#[test]
fn json_last_error_case_insensitive() {
    let out = compile_and_run(
        r#"<?php json_decode("not json"); $e = Json_Last_Error(); echo $e > 0 ? "err" : "ok";"#,
    );
    assert_eq!(out, "err");
}

/// Verifies json_last_error_msg is reachable via the root namespace escape (\json_last_error_msg).
#[test]
fn json_last_error_msg_namespaced() {
    let out = compile_and_run(
        r#"<?php json_decode("not json"); echo strlen(\json_last_error_msg()) > 0 ? "msg" : "empty";"#,
    );
    assert_eq!(out, "msg");
}
