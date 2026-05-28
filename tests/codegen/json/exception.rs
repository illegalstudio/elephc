//! Purpose:
//! Provides JsonException hierarchy and code tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Thrown JSON exceptions must be catchable through the PHP exception hierarchy.

use super::*;

/// Verifies JsonException can be constructed with a message and the message is retrievable.
#[test]
fn test_json_exception_construct_and_get_message() {
    let out = compile_and_run(
        r#"<?php $e = new JsonException("decode failed"); echo $e->getMessage();"#,
    );
    assert_eq!(out, "decode failed");
}

/// Verifies RuntimeException can be constructed with a message and the message is retrievable.
#[test]
fn test_runtime_exception_construct_and_get_message() {
    let out = compile_and_run(
        r#"<?php $e = new RuntimeException("rte"); echo $e->getMessage();"#,
    );
    assert_eq!(out, "rte");
}

/// Verifies JsonException is catchable as JsonException via a typed catch clause.
#[test]
fn test_json_exception_caught_as_json_exception() {
    let out = compile_and_run(
        r#"<?php
try { throw new JsonException("decode failed"); }
catch (JsonException $e) { echo "caught: " . $e->getMessage(); }
"#,
    );
    assert_eq!(out, "caught: decode failed");
}

/// Verifies JsonException is catchable as RuntimeException (parent class).
#[test]
fn test_json_exception_caught_as_runtime_exception() {
    let out = compile_and_run(
        r#"<?php
try { throw new JsonException("again"); }
catch (RuntimeException $e) { echo "rte: " . $e->getMessage(); }
"#,
    );
    assert_eq!(out, "rte: again");
}

/// Verifies JsonException is catchable as Exception (root of exception hierarchy).
#[test]
fn test_json_exception_caught_as_exception() {
    let out = compile_and_run(
        r#"<?php
try { throw new JsonException("third"); }
catch (Exception $e) { echo "ex: " . $e->getMessage(); }
"#,
    );
    assert_eq!(out, "ex: third");
}

/// Verifies JsonException is an instanceof RuntimeException.
#[test]
fn test_json_exception_instanceof_runtime_exception() {
    let out = compile_and_run(
        r#"<?php
$e = new JsonException("x");
echo ($e instanceof RuntimeException ? "yes" : "no");
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies JsonException is an instanceof Exception.
#[test]
fn test_json_exception_instanceof_exception() {
    let out = compile_and_run(
        r#"<?php
$e = new JsonException("x");
echo ($e instanceof Exception ? "yes" : "no");
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies JsonException is an instanceof Throwable.
#[test]
fn test_json_exception_instanceof_throwable() {
    let out = compile_and_run(
        r#"<?php
$e = new JsonException("x");
echo ($e instanceof Throwable ? "yes" : "no");
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies RuntimeException is an instanceof Exception.
#[test]
fn test_runtime_exception_instanceof_exception() {
    let out = compile_and_run(
        r#"<?php
$e = new RuntimeException("x");
echo ($e instanceof Exception ? "yes" : "no");
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies a plain Exception is NOT an instanceof JsonException.
#[test]
fn test_plain_exception_is_not_json_exception() {
    let out = compile_and_run(
        r#"<?php
$e = new Exception("plain");
echo ($e instanceof JsonException ? "yes" : "no");
"#,
    );
    assert_eq!(out, "no");
}

// JsonException::getCode() — the JSON_ERROR_* code that triggered the throw
// is exposed via Exception's standard $code property and getCode() accessor.

/// Verifies JsonException::getCode() returns JSON_ERROR_SYNTAX (4) for malformed JSON input.
#[test]
fn test_json_exception_get_code_syntax() {
    let out = compile_and_run(
        r#"<?php
            try { json_decode("invalid", null, 512, JSON_THROW_ON_ERROR); echo "no throw"; }
            catch (JsonException $e) { echo $e->getCode(); }
        "#,
    );
    assert_eq!(out, "4");
}

/// Verifies JsonException::getCode() returns JSON_ERROR_DEPTH (1) for depth limit exceeded.
#[test]
fn test_json_exception_get_code_depth() {
    let out = compile_and_run(
        r#"<?php
            try { json_decode("[[1]]", false, 1, JSON_THROW_ON_ERROR); echo "no throw"; }
            catch (JsonException $e) { echo $e->getCode(); }
        "#,
    );
    assert_eq!(out, "1");
}

/// Verifies JsonException::getCode() returns JSON_ERROR_UTF16 (10) for lone UTF-16 surrogate.
#[test]
fn test_json_exception_get_code_utf16() {
    let out = compile_and_run(
        r#"<?php
            try { json_decode("\"\\uD83D\"", null, 512, JSON_THROW_ON_ERROR); echo "no throw"; }
            catch (JsonException $e) { echo $e->getCode(); }
        "#,
    );
    assert_eq!(out, "10");
}

/// Verifies JsonException::getCode() returns JSON_ERROR_INF_OR_NAN (7) for encoding INF or NAN.
#[test]
fn test_json_exception_get_code_inf_or_nan() {
    let out = compile_and_run(
        r#"<?php
            try { json_encode(INF, JSON_THROW_ON_ERROR); echo "no throw"; }
            catch (JsonException $e) { echo $e->getCode(); }
        "#,
    );
    assert_eq!(out, "7");
}

/// Verifies Exception constructor accepts an optional integer code argument.
#[test]
fn test_exception_get_code_user_constructor() {
    let out = compile_and_run(
        r#"<?php
            $e = new Exception("hi", 42);
            echo $e->getCode();
        "#,
    );
    assert_eq!(out, "42");
}

/// Verifies Exception defaults to code 0 when no second argument is provided.
#[test]
fn test_exception_get_code_default_zero() {
    let out = compile_and_run(
        r#"<?php
            $e = new Exception("hi");
            echo $e->getCode();
        "#,
    );
    assert_eq!(out, "0");
}
