//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of type-related builtins strict comparison semantics, including strict equality integer same, strict equality integer different, and strict inequality integer same.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies `===` returns true for identical integer values.
#[test]
fn test_strict_eq_int_same() {
    let out = compile_and_run("<?php echo 1 === 1;");
    assert_eq!(out, "1");
}

/// Verifies `===` returns empty (false) when comparing two different integer values.
#[test]
fn test_strict_eq_int_different() {
    let out = compile_and_run("<?php echo 1 === 2;");
    assert_eq!(out, "");
}

/// Verifies `!==` returns empty (false) when comparing two identical integer values.
#[test]
fn test_strict_neq_int_same() {
    let out = compile_and_run("<?php echo 1 !== 1;");
    assert_eq!(out, "");
}

/// Verifies `!==` returns true for two different integer values.
#[test]
fn test_strict_neq_int_different() {
    let out = compile_and_run("<?php echo 1 !== 2;");
    assert_eq!(out, "1");
}

/// Verifies `===` returns false when comparing int `1` to bool `true` (different types).
#[test]
fn test_strict_eq_int_vs_bool() {
    let out = compile_and_run("<?php echo 1 === true;");
    assert_eq!(out, "");
}

/// Verifies `!==` returns true when comparing int `1` to bool `true` (different types).
#[test]
fn test_strict_neq_int_vs_bool() {
    let out = compile_and_run("<?php echo 1 !== true;");
    assert_eq!(out, "1");
}

/// Verifies `===` returns false when comparing int `1` to string `"1"` (different types).
#[test]
fn test_strict_eq_int_vs_string() {
    let out = compile_and_run("<?php echo 1 === \"1\";");
    assert_eq!(out, "");
}

/// Verifies `===` returns true for two identical string values.
#[test]
fn test_strict_eq_string_same() {
    let out = compile_and_run("<?php echo \"hello\" === \"hello\";");
    assert_eq!(out, "1");
}

/// Verifies `===` returns empty (false) when comparing two different string values.
#[test]
fn test_strict_eq_string_different() {
    let out = compile_and_run("<?php echo \"hello\" === \"world\";");
    assert_eq!(out, "");
}

/// Verifies `!==` returns true for two different string values.
#[test]
fn test_strict_neq_string() {
    let out = compile_and_run("<?php echo \"abc\" !== \"def\";");
    assert_eq!(out, "1");
}

/// Verifies `===` returns true when both operands are boolean `true`.
#[test]
fn test_strict_eq_bool_true() {
    let out = compile_and_run("<?php echo true === true;");
    assert_eq!(out, "1");
}

/// Verifies `===` returns true when both operands are boolean `false`.
#[test]
fn test_strict_eq_bool_false() {
    let out = compile_and_run("<?php echo false === false;");
    assert_eq!(out, "1");
}

/// Verifies `===` returns empty (false) when comparing `true` to `false`.
#[test]
fn test_strict_eq_bool_mixed() {
    let out = compile_and_run("<?php echo true === false;");
    assert_eq!(out, "");
}

/// Verifies `===` returns true when both operands are `null`.
#[test]
fn test_strict_eq_null() {
    let out = compile_and_run("<?php echo null === null;");
    assert_eq!(out, "1");
}

/// Verifies `===` returns false when comparing `null` to integer `0` (different types).
#[test]
fn test_strict_eq_null_vs_int() {
    let out = compile_and_run("<?php echo null === 0;");
    assert_eq!(out, "");
}

/// Verifies `===` returns false when comparing `null` to bool `false` (different types).
#[test]
fn test_strict_eq_null_vs_false() {
    let out = compile_and_run("<?php echo null === false;");
    assert_eq!(out, "");
}

/// Verifies `===` returns true for two identical float values.
#[test]
fn test_strict_eq_float_same() {
    let out = compile_and_run("<?php echo 3.14 === 3.14;");
    assert_eq!(out, "1");
}

/// Verifies `===` returns empty (false) when comparing two different float values.
#[test]
fn test_strict_eq_float_different() {
    let out = compile_and_run("<?php echo 3.14 === 2.71;");
    assert_eq!(out, "");
}

/// Verifies `===` returns false when comparing float `1.0` to int `1` (different types).
#[test]
fn test_strict_eq_float_vs_int() {
    let out = compile_and_run("<?php echo 1.0 === 1;");
    assert_eq!(out, "");
}

/// Verifies `===` works correctly inside an `if` condition with an integer variable.
#[test]
fn test_strict_eq_in_if() {
    let out = compile_and_run(
        r#"<?php
$x = 5;
if ($x === 5) {
    echo "yes";
} else {
    echo "no";
}
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies `!==` works correctly inside an `if` condition with string variables.
#[test]
fn test_strict_neq_in_if() {
    let out = compile_and_run(
        r#"<?php
$x = "hello";
if ($x !== "world") {
    echo "different";
} else {
    echo "same";
}
"#,
    );
    assert_eq!(out, "different");
}

/// Verifies `===` returns true when two distinct variables hold the same string value.
#[test]
fn test_strict_eq_string_variables() {
    let out = compile_and_run(
        r#"<?php
$a = "test";
$b = "test";
echo $a === $b;
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies `!==` returns true when two distinct variables hold different string values.
#[test]
fn test_strict_neq_string_variables() {
    let out = compile_and_run(
        r#"<?php
$a = "foo";
$b = "bar";
echo $a !== $b;
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies both operands of `===` are evaluated even when types differ (no short-circuit on type mismatch).
#[test]
fn test_strict_eq_side_effects_preserved() {
    let out = compile_and_run(
        r#"<?php
function effect() { echo "X"; return 1; }
$r = 1.0 === effect();
echo $r;
"#,
    );
    assert_eq!(out, "X");
}

/// Verifies the boolean result of `===` can be assigned to a variable.
#[test]
fn test_strict_eq_assign_result() {
    let out = compile_and_run(
        r#"<?php
$x = 1 === 1;
echo $x;
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies the boolean result of `!==` can be assigned to a variable.
#[test]
fn test_strict_neq_assign_result() {
    let out = compile_and_run(
        r#"<?php
$x = 1 !== 2;
echo $x;
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies strict comparison uses both type and value from a map with int, string, and bool entries.
#[test]
fn test_strict_compare_mixed_uses_payload_type_and_value() {
    let out = compile_and_run(
        r#"<?php
$map = [
    "int_a" => 42,
    "int_b" => 42,
    "int_c" => 7,
    "str_a" => "42",
    "str_b" => "42",
    "bool_t" => true,
];
echo $map["int_a"] === $map["int_b"] ? "1" : "0";
echo $map["int_a"] === $map["int_c"] ? "1" : "0";
echo $map["int_a"] === $map["str_a"] ? "1" : "0";
echo $map["str_a"] === $map["str_b"] ? "1" : "0";
echo $map["int_a"] !== $map["str_a"] ? "1" : "0";
echo $map["bool_t"] === true ? "1" : "0";
"#,
    );
    assert_eq!(out, "100111");
}

// --- Include / Require ---
