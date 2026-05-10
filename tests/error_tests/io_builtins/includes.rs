//! Purpose:
//! Integration or regression tests for diagnostic coverage of I/O builtin includes, including require once chain preserves included file error location, include missing path, and include non string path.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Multi-file fixtures exercise include/require resolution, temporary project layout, and native binary output.

use super::*;

#[test]
fn test_require_once_chain_preserves_included_file_error_location() {
    let err = resolve_files_error(
        &[
            ("main.php", "<?php\nrequire_once 'a.php';\n"),
            ("a.php", "<?php\nrequire_once 'nested/b.php';\n"),
            ("nested/b.php", "<?php\nfunction broken() {\n    echo 1\n}\n"),
        ],
        "main.php",
    );

    assert_eq!(err.span.line, 4, "expected parser error to point into nested/b.php");
    assert_ne!(err.span.line, 2, "error should not point back to the require_once line");
    assert!(
        Path::new(err.file.as_deref().expect("expected included file path")).ends_with("nested/b.php"),
        "expected file path to reference nested/b.php, got {:?}",
        err.file,
    );
    assert!(
        err.message.contains("Expected ';'"),
        "unexpected error message: {}",
        err.message,
    );
    assert!(
        err.to_string().contains("nested/b.php:4"),
        "expected display output to include nested/b.php:4, got {}",
        err,
    );
}

// --- Float/math function errors ---

#[test]
fn test_error_include_missing_path() {
    // Empty `include ;` — parse_expr immediately sees `;` and errors out.
    expect_error("<?php include ;", "Unexpected token");
}

#[test]
fn test_error_include_non_string_path() {
    // Non-foldable path — parses fine but the resolver rejects it because
    // an integer literal is not a compile-time-constant *string*.
    let err = resolver_error("<?php include 42;");
    assert!(
        err.message.contains("compile-time-constant string"),
        "message did not mention compile-time-constant string: {}",
        err.message
    );
    assert!(
        !err.message.contains("Runtime-dynamic"),
        "static non-string path should not be reported as runtime-dynamic: {}",
        err.message
    );
}

fn expect_runtime_dynamic_include_path_error(src: &str, expected_detail: &str) {
    let err = resolver_error(src);
    assert!(
        err.message
            .contains("Runtime-dynamic include/require path expressions are not supported"),
        "message did not reject runtime-dynamic include paths: {}",
        err.message
    );
    assert!(
        err.message.contains(expected_detail),
        "message '{}' did not contain '{}'",
        err.message,
        expected_detail
    );
}

#[test]
fn test_error_include_variable_path_is_explicitly_rejected() {
    expect_runtime_dynamic_include_path_error(
        "<?php function load($path) { require $path; }",
        "variable `$path` is resolved at runtime",
    );
}

#[test]
fn test_error_include_concat_variable_path_reports_runtime_part() {
    expect_runtime_dynamic_include_path_error(
        "<?php function load($path) { require 'lib/' . $path; }",
        "variable `$path` is resolved at runtime",
    );
}

#[test]
fn test_error_include_once_variable_path_is_explicitly_rejected() {
    expect_runtime_dynamic_include_path_error(
        "<?php function load($path) { include_once $path; }",
        "variable `$path` is resolved at runtime",
    );
}

#[test]
fn test_error_include_function_call_path_is_explicitly_rejected() {
    expect_runtime_dynamic_include_path_error(
        "<?php function pick() { return 'helper.php'; } require pick();",
        "function call `pick()` is resolved at runtime",
    );
}

#[test]
fn test_error_include_ternary_path_is_explicitly_rejected() {
    expect_runtime_dynamic_include_path_error(
        "<?php require true ? 'debug.php' : 'prod.php';",
        "ternary path selection is resolved at runtime",
    );
}

#[test]
fn test_error_include_property_path_is_explicitly_rejected() {
    expect_runtime_dynamic_include_path_error(
        "<?php require $config->path;",
        "property access `->path` is resolved at runtime",
    );
}

// --- INF/NAN function errors ---

