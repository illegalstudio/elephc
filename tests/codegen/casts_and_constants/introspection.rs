//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of casts, constants, and introspection introspection, including gettype integer, gettype float, and gettype string.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests that `gettype(42)` returns "integer".
#[test]
fn test_gettype_int() {
    let out = compile_and_run("<?php echo gettype(42);");
    assert_eq!(out, "integer");
}

/// Tests that `gettype(3.14)` returns "double" (PHP's float type name).
#[test]
fn test_gettype_float() {
    let out = compile_and_run("<?php echo gettype(3.14);");
    assert_eq!(out, "double");
}

/// Tests that `gettype("hi")` returns "string".
#[test]
fn test_gettype_string() {
    let out = compile_and_run("<?php echo gettype(\"hi\");");
    assert_eq!(out, "string");
}

/// Tests that `gettype(true)` returns "boolean".
#[test]
fn test_gettype_bool() {
    let out = compile_and_run("<?php echo gettype(true);");
    assert_eq!(out, "boolean");
}

/// Tests that `gettype(null)` returns "NULL".
#[test]
fn test_gettype_null() {
    let out = compile_and_run("<?php echo gettype(null);");
    assert_eq!(out, "NULL");
}

/// Tests that `gettype` on a mixed value returns the concrete payload type
/// (integer, string, NULL, array, boolean) rather than "mixed".
#[test]
fn test_gettype_mixed_returns_concrete_payload_type() {
    let out = compile_and_run(
        r#"<?php
$map = [
    "i" => 42,
    "s" => "hi",
    "n" => null,
    "a" => [1, 2],
    "b" => true,
];
echo gettype($map["i"]);
echo "|";
echo gettype($map["s"]);
echo "|";
echo gettype($map["n"]);
echo "|";
echo gettype($map["a"]);
echo "|";
echo gettype($map["b"]);
"#,
    );
    assert_eq!(out, "integer|string|NULL|array|boolean");
}

// --- empty ---

/// Tests that `empty(0)` is true (0 is falsy in PHP).
#[test]
fn test_empty_zero() {
    let out = compile_and_run("<?php echo empty(0);");
    assert_eq!(out, "1");
}

/// Tests that `empty(42)` is false (non-zero int is truthy).
#[test]
fn test_empty_nonzero() {
    let out = compile_and_run("<?php echo empty(42);");
    assert_eq!(out, "");
}

/// Tests that `empty("")` is true (empty string is falsy).
#[test]
fn test_empty_empty_string() {
    let out = compile_and_run("<?php echo empty(\"\");");
    assert_eq!(out, "1");
}

/// Tests that `empty("hi")` is false (non-empty string is truthy).
#[test]
fn test_empty_nonempty_string() {
    let out = compile_and_run("<?php echo empty(\"hi\");");
    assert_eq!(out, "");
}

/// Tests that `empty(null)` is true.
#[test]
fn test_empty_null() {
    let out = compile_and_run("<?php echo empty(null);");
    assert_eq!(out, "1");
}

/// Tests that `empty(false)` is true.
#[test]
fn test_empty_false() {
    let out = compile_and_run("<?php echo empty(false);");
    assert_eq!(out, "1");
}

/// Tests that `empty(true)` is false.
#[test]
fn test_empty_true() {
    let out = compile_and_run("<?php echo empty(true);");
    assert_eq!(out, "");
}

/// Tests that `empty` on a mixed-valued associative array uses boxed payload
/// semantics (zeros/blank/null/empty-array are falsy; non-zeros/non-blank are truthy).
#[test]
fn test_empty_mixed_uses_boxed_payload_semantics() {
    let out = compile_and_run(
        r#"<?php
$map = [
    "zero" => 0,
    "blank" => "",
    "null" => null,
    "arr" => [],
    "one" => 1,
    "text" => "hi",
];
echo empty($map["zero"]) ? "1" : "0";
echo empty($map["blank"]) ? "1" : "0";
echo empty($map["null"]) ? "1" : "0";
echo empty($map["arr"]) ? "1" : "0";
echo empty($map["one"]) ? "1" : "0";
echo empty($map["text"]) ? "1" : "0";
"#,
    );
    assert_eq!(out, "111100");
}

// --- unset ---

/// Tests that `unset` marks a variable as undefined so `is_null` returns true.
#[test]
fn test_unset_variable() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
unset($x);
echo is_null($x);
"#,
    );
    assert_eq!(out, "1");
}

// --- settype ---

/// Tests that `settype($x, "string")` converts an integer to a string.
#[test]
fn test_settype_to_string() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
settype($x, "string");
echo $x;
"#,
    );
    assert_eq!(out, "42");
}

/// Tests that `settype($x, "integer")` truncates a float to an integer.
#[test]
fn test_settype_to_int() {
    let out = compile_and_run(
        r#"<?php
$x = 3.7;
settype($x, "integer");
echo $x;
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies settype() coerces string and boxed Mixed sources per PHP cast rules (L2): a string
/// or Mixed source was previously zeroed for "integer"/"float" and mis-handled for "bool".
#[test]
fn test_settype_string_and_mixed_sources() {
    let out = compile_and_run(
        r#"<?php
$i = "42abc"; settype($i, "integer");
$f = "3.14"; settype($f, "float");
$b0 = "0";   settype($b0, "bool");
$b1 = "hi";  settype($b1, "bool");
$a = ["7.5", 1]; $m = $a[0]; settype($m, "float");
$ok = $i === 42 && $f === 3.14 && $b0 === false && $b1 === true && $m === 7.5;
echo $ok ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

// --- Missing type function tests ---
