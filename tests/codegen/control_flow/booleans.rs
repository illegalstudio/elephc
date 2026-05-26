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
    // Post-increment `$x++` three times from 0 yields 3.
    let out = compile_and_run("<?php $x = 0; $x++; $x++; $x++; echo $x;");
    assert_eq!(out, "3");
}

#[test]
fn test_standalone_decrement() {
    // Post-decrement `$x--` twice from 10 yields 8.
    let out = compile_and_run("<?php $x = 10; $x--; $x--; echo $x;");
    assert_eq!(out, "8");
}

#[test]
fn test_and_true() {
    // Logical AND of two true values yields 1 (true prints "1").
    let out = compile_and_run("<?php echo 1 && 1;");
    assert_eq!(out, "1");
}

#[test]
fn test_and_false() {
    // Logical AND with false right operand yields empty (false prints nothing).
    let out = compile_and_run("<?php echo 1 && 0;");
    assert_eq!(out, "");
}

#[test]
fn test_or_true() {
    // Logical OR with true right operand yields 1 (PHP true is "1").
    let out = compile_and_run("<?php echo 0 || 1;");
    assert_eq!(out, "1");
}

#[test]
fn test_or_false() {
    // Logical OR of two false values yields empty (false prints nothing).
    let out = compile_and_run("<?php echo 0 || 0;");
    assert_eq!(out, "");
}

#[test]
fn test_not_zero() {
    // Negation of zero yields true ("1").
    let out = compile_and_run("<?php $x = 0; echo !$x;");
    assert_eq!(out, "1");
}

#[test]
fn test_not_nonzero() {
    // Negation of non-zero value (42) yields false (empty string).
    let out = compile_and_run("<?php $x = 42; echo !$x;");
    assert_eq!(out, "");
}

#[test]
fn test_short_circuit_and() {
    // `0 && inc()` short-circuits: left operand is false, so RHS is never evaluated.
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
    // `1 || inc()` short-circuits: left operand is true, so RHS is never evaluated; result is "1".
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
    // `(false and mark())` short-circuits: word AND does not evaluate RHS when left is false.
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
    // `(true or mark())` short-circuits: word OR does not evaluate RHS when left is true.
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
    // `xor` is non-short-circuiting: both operands are always evaluated.
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
    // Word logical operators (AND, Or, xOr) are case-insensitive keywords.
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
    // Word operators (and, or, xor) have lower precedence than symbolic (&&, ||); verifies correct parsing of mixed expressions.
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
    // `echo true` outputs "1" (PHP booleans print as "1" or empty string).
    let out = compile_and_run("<?php echo true;");
    assert_eq!(out, "1");
}

#[test]
fn test_boolean_false() {
    // `echo false` outputs empty string.
    let out = compile_and_run("<?php echo false;");
    assert_eq!(out, "");
}

#[test]
fn test_boolean_in_condition() {
    // Boolean literals in `if` conditions: `if (true)` executes, `if (false)` skips.
    let out = compile_and_run("<?php if (true) { echo \"yes\"; } if (false) { echo \"no\"; }");
    assert_eq!(out, "yes");
}

// --- Assignment operators ---

#[test]
fn test_logical_with_comparison() {
    // Logical AND combines two comparison expressions: `$x > 3 && $x < 10` is true for $x = 5.
    let out = compile_and_run("<?php $x = 5; echo ($x > 3 && $x < 10);");
    assert_eq!(out, "1");
}

// --- Logical operators with null ---

#[test]
fn test_null_and_true() {
    // `null` is falsy; `null && true` evaluates to false (empty string).
    let out = compile_and_run("<?php echo null && true;");
    assert_eq!(out, "");
}

#[test]
fn test_true_and_null() {
    // `null` is falsy; `true && null` evaluates to false (empty string).
    let out = compile_and_run("<?php echo true && null;");
    assert_eq!(out, "");
}

#[test]
fn test_null_or_false() {
    // `null` is falsy; `null || false` evaluates to false (empty string).
    let out = compile_and_run("<?php echo null || false;");
    assert_eq!(out, "");
}

#[test]
fn test_false_or_null() {
    // `false || null` evaluates to null; both operands are falsy, null propagates.
    let out = compile_and_run("<?php echo false || null;");
    assert_eq!(out, "");
}

#[test]
fn test_null_or_true() {
    // `null || true` evaluates to true; RHS is returned when LHS is falsy.
    let out = compile_and_run("<?php echo null || true;");
    assert_eq!(out, "1");
}

#[test]
fn test_null_and_false() {
    // `null && false` evaluates to false (empty string); both operands are falsy.
    let out = compile_and_run("<?php echo null && false;");
    assert_eq!(out, "");
}

#[test]
fn test_null_var_and() {
    // `$x = null; $x && true` evaluates to false; null is falsy.
    let out = compile_and_run("<?php $x = null; echo $x && true;");
    assert_eq!(out, "");
}

#[test]
fn test_null_var_or() {
    // `$x = null; $x || false` evaluates to false; both operands are falsy, null propagates.
    let out = compile_and_run("<?php $x = null; echo $x || false;");
    assert_eq!(out, "");
}

#[test]
fn test_not_null_is_true() {
    // Negation of null yields true; `!null` is true ("1").
    let out = compile_and_run("<?php $x = null; echo !$x;");
    assert_eq!(out, "1");
}

#[test]
fn test_short_ternary_truthy_and_falsy_values() {
    // Elvis operator `?:` returns left for truthy values (5, "hi", "0") and right for falsy (0, "", null).
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
    // Elvis operator `?:` evaluates the left operand exactly once; static counter increments per call.
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
    // Elvis operator `?:` is short-circuiting: RHS (fallback) is only evaluated when LHS is falsy.
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
