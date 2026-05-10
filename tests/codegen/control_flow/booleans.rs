//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow booleans, including standalone increment, standalone decrement, and and true.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_standalone_increment() {
    let out = compile_and_run("<?php $x = 0; $x++; $x++; $x++; echo $x;");
    assert_eq!(out, "3");
}

#[test]
fn test_standalone_decrement() {
    let out = compile_and_run("<?php $x = 10; $x--; $x--; echo $x;");
    assert_eq!(out, "8");
}

#[test]
fn test_and_true() {
    let out = compile_and_run("<?php echo 1 && 1;");
    assert_eq!(out, "1");
}

#[test]
fn test_and_false() {
    let out = compile_and_run("<?php echo 1 && 0;");
    assert_eq!(out, "");
}

#[test]
fn test_or_true() {
    let out = compile_and_run("<?php echo 0 || 1;");
    assert_eq!(out, "1");
}

#[test]
fn test_or_false() {
    let out = compile_and_run("<?php echo 0 || 0;");
    assert_eq!(out, "");
}

#[test]
fn test_not_zero() {
    let out = compile_and_run("<?php $x = 0; echo !$x;");
    assert_eq!(out, "1");
}

#[test]
fn test_not_nonzero() {
    let out = compile_and_run("<?php $x = 42; echo !$x;");
    assert_eq!(out, "");
}

#[test]
fn test_short_circuit_and() {
    let out = compile_and_run(
        r#"<?php
$count = 0;
function inc() { return 1; }
$r = 0 && inc();
echo $r;
"#,
    );
    assert_eq!(out, ""); // false prints nothing
}

#[test]
fn test_short_circuit_or() {
    // With ||, if left is true the right side should not be evaluated.
    let out = compile_and_run(
        r#"<?php
function inc() { return 1; }
$r = 1 || inc();
echo $r;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_word_and_short_circuit() {
    let out = compile_and_run(
        r#"<?php
function mark() { echo "rhs"; return true; }
$r = (false and mark());
echo $r ? "T" : "F";
"#,
    );
    assert_eq!(out, "F");
}

#[test]
fn test_word_or_short_circuit() {
    let out = compile_and_run(
        r#"<?php
function mark() { echo "rhs"; return false; }
$r = (true or mark());
echo $r ? "T" : "F";
"#,
    );
    assert_eq!(out, "T");
}

#[test]
fn test_word_xor_evaluates_rhs() {
    let out = compile_and_run(
        r#"<?php
function mark() { echo "rhs"; return false; }
$r = (true xor mark());
echo $r ? "T" : "F";
"#,
    );
    assert_eq!(out, "rhsT");
}

#[test]
fn test_word_logical_operators_are_case_insensitive_codegen() {
    let out = compile_and_run(
        r#"<?php
echo (true AND false) ? "T" : "F";
echo (false Or true) ? "T" : "F";
echo (true xOr false) ? "T" : "F";
"#,
    );
    assert_eq!(out, "FTT");
}

#[test]
fn test_word_logical_precedence_against_symbolic_logical() {
    let out = compile_and_run(
        r#"<?php
$a = (true || false and false);
echo $a ? "T" : "F";
$b = (false && true or true);
echo $b ? "T" : "F";
$c = (true xor true and false);
echo $c ? "T" : "F";
"#,
    );
    assert_eq!(out, "FTT");
}

#[test]
fn test_boolean_true() {
    let out = compile_and_run("<?php echo true;");
    assert_eq!(out, "1");
}

#[test]
fn test_boolean_false() {
    let out = compile_and_run("<?php echo false;");
    assert_eq!(out, "");
}

#[test]
fn test_boolean_in_condition() {
    let out = compile_and_run("<?php if (true) { echo \"yes\"; } if (false) { echo \"no\"; }");
    assert_eq!(out, "yes");
}

// --- Assignment operators ---

#[test]
fn test_logical_with_comparison() {
    let out = compile_and_run("<?php $x = 5; echo ($x > 3 && $x < 10);");
    assert_eq!(out, "1");
}

// --- Logical operators with null ---

#[test]
fn test_null_and_true() {
    // null && true → false (null coerces to false)
    let out = compile_and_run("<?php echo null && true;");
    assert_eq!(out, "");
}

#[test]
fn test_true_and_null() {
    let out = compile_and_run("<?php echo true && null;");
    assert_eq!(out, "");
}

#[test]
fn test_null_or_false() {
    // null || false → false
    let out = compile_and_run("<?php echo null || false;");
    assert_eq!(out, "");
}

#[test]
fn test_false_or_null() {
    let out = compile_and_run("<?php echo false || null;");
    assert_eq!(out, "");
}

#[test]
fn test_null_or_true() {
    // null || true → true
    let out = compile_and_run("<?php echo null || true;");
    assert_eq!(out, "1");
}

#[test]
fn test_null_and_false() {
    let out = compile_and_run("<?php echo null && false;");
    assert_eq!(out, "");
}

#[test]
fn test_null_var_and() {
    let out = compile_and_run("<?php $x = null; echo $x && true;");
    assert_eq!(out, "");
}

#[test]
fn test_null_var_or() {
    let out = compile_and_run("<?php $x = null; echo $x || false;");
    assert_eq!(out, "");
}

#[test]
fn test_not_null_is_true() {
    // !null → true
    let out = compile_and_run("<?php $x = null; echo !$x;");
    assert_eq!(out, "1");
}

#[test]
fn test_short_ternary_truthy_and_falsy_values() {
    let out = compile_and_run(
        r#"<?php
echo (5 ?: 9) . ":";
echo (0 ?: 9) . ":";
echo ("hi" ?: "fallback") . ":";
echo ("" ?: "empty") . ":";
echo ("0" ?: "zero") . ":";
echo (null ?: "null");
"#,
    );
    assert_eq!(out, "5:9:hi:empty:zero:null");
}

#[test]
fn test_short_ternary_evaluates_left_once() {
    let out = compile_and_run(
        r#"<?php
function next_value() {
    static $n = 0;
    $n++;
    return $n;
}
echo (next_value() ?: 99);
echo ":";
echo next_value();
"#,
    );
    assert_eq!(out, "1:2");
}

#[test]
fn test_short_ternary_rhs_only_evaluates_when_left_is_falsy() {
    let out = compile_and_run(
        r#"<?php
function fallback() {
    echo "rhs";
    return 7;
}
echo (1 ?: fallback());
echo ":";
echo (0 ?: fallback());
"#,
    );
    assert_eq!(out, "1:rhs7");
}
