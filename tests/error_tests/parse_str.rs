//! Purpose:
//! Checks `parse_str` diagnostics for missing arguments and a non-variable result.
//!
//! Called from:
//! - The `error_tests` integration-test root through Rust's test harness.
//!
//! Key details:
//! - `parse_str` is a shared PreludeProvided contract injected before type checking.
//! - The second parameter is untyped by reference, so a literal is rejected at the call boundary.

use super::*;

/// Rejects a call without the required string and result arguments.
#[test]
fn parse_str_requires_two_arguments() {
    let error = check_source("<?php parse_str();").expect_err("parse_str without arguments must fail");
    assert!(error.contains("parse_str"), "{error}");
    assert!(error.contains("argument"), "{error}");
}

/// Rejects a literal second argument because the result parameter is passed by reference.
#[test]
fn parse_str_result_must_be_a_variable() {
    let error = check_source("<?php parse_str('a=1', 5);")
        .expect_err("parse_str must reject a non-variable result");
    assert!(error.contains("parse_str"), "{error}");
    assert!(error.contains("variable") || error.contains("reference"), "{error}");
}
