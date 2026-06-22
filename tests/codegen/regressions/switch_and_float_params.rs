//! Purpose:
//! Regression tests for two general EIR lowering bugs:
//! 1. A `switch` over a string subject collapsed every case to `0 == 0`, so it
//!    always took the first case (the subject and each case were `coerce_to_int`'d,
//!    turning non-numeric strings into 0).
//! 2. An `int`/`bool` argument passed to a `float` parameter was deposited in an
//!    integer register and read back as garbage from a floating-point slot, because
//!    no int→float widening happened at the call boundary.
//! 3. Loose equality between a float and an int (`1.5 == 1`, and `switch (1.5)`)
//!    either failed to compile (`loose_eq for PHP types Float and Int` was an
//!    unsupported backend feature) or truncated the float subject to int in the
//!    dynamic switch dispatch, so `switch (1.5) { case 1.5; }` wrongly matched
//!    `case 1`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Bug 1 fix: the dynamic switch dispatch compares with PHP loose equality
//!   (`Op::LooseEq`) for string subjects/cases; the integer jump table is reserved
//!   for genuinely integer-typed subjects.
//! - Bug 2 fix: `coerce_operands_to_params` widens int/bool operands bound to pure
//!   `float` parameters before the call is emitted.
//! - Bug 3 fix: `lower_loose_eq` promotes a float-vs-int pair to float and compares
//!   numerically, and the dynamic switch routes float/numeric pairs through
//!   `Op::LooseEq` instead of the int jump path. These tests cover statically-typed
//!   float operands; an untyped (`Mixed`) float subject is a separate, broader
//!   loose-equality limitation (issue #397) and is intentionally not asserted here.

use crate::support::*;

/// A `switch` over a string matches the correct case rather than always the first.
#[test]
fn test_string_switch_matches_correct_case() {
    let out = compile_and_run(
        r#"<?php
function classify(string $s): int {
    switch ($s) {
        case "black": return 10;
        case "white": return 20;
        case "red":   return 30;
        default:      return 99;
    }
}
echo classify("black"), classify("white"), classify("red"), classify("green");
"#,
    );
    assert_eq!(out, "10203099");
}

/// String switches honor comma-separated case labels and PHP fallthrough.
#[test]
fn test_string_switch_fallthrough_and_multilabel() {
    let out = compile_and_run(
        r#"<?php
function grp(string $s): string {
    switch ($s) {
        case "a":
        case "b": return "ab";
        case "c": return "c";
        default:  return "?";
    }
}
echo grp("a"), grp("b"), grp("c"), grp("z");
"#,
    );
    assert_eq!(out, "ababc?");
}

/// A string switch with a non-default first case still falls through to default
/// when nothing matches (proves the subject is not silently coerced to the first).
#[test]
fn test_string_switch_default_when_no_match() {
    let out = compile_and_run(
        r#"<?php
$s = "zzz";
switch ($s) {
    case "one": echo "1"; break;
    case "two": echo "2"; break;
    default:    echo "D"; break;
}
"#,
    );
    assert_eq!(out, "D");
}

/// The integer jump-table path is preserved for integer-typed subjects.
#[test]
fn test_int_switch_jump_table_still_works() {
    let out = compile_and_run(
        r#"<?php
function isw(int $n): string {
    switch ($n) {
        case 1: return "one";
        case 2: return "two";
        default: return "many";
    }
}
echo isw(1), isw(2), isw(9);
"#,
    );
    assert_eq!(out, "onetwomany");
}

/// An int argument passed to a `float` parameter is widened, not read as garbage.
#[test]
fn test_int_arg_to_float_param_widens() {
    let out = compile_and_run(
        r#"<?php
function takesFloat(float $x): float { return $x + 0.5; }
echo takesFloat(3), " ", takesFloat(10), " ", takesFloat(3.0);
"#,
    );
    assert_eq!(out, "3.5 10.5 3.5");
}

/// Int→float widening applies to instance-method parameters too.
#[test]
fn test_int_arg_to_float_method_param_widens() {
    let out = compile_and_run(
        r#"<?php
class Calc {
    public function scale(float $f): float { return $f * 2.0; }
}
$c = new Calc();
echo $c->scale(5);
"#,
    );
    assert_eq!(out, "10");
}

/// Int defaults and named int arguments bound to float parameters widen correctly.
#[test]
fn test_int_default_and_named_float_params_widen() {
    let out = compile_and_run(
        r#"<?php
function withDefault(float $x = 7): float { return $x; }
function named(float $a, float $b): float { return $a - $b; }
echo withDefault(), " ", named(b: 2, a: 10);
"#,
    );
    assert_eq!(out, "7 8");
}

/// A bool argument bound to a float parameter widens to 1.0 / 0.0 per PHP.
#[test]
fn test_bool_arg_to_float_param_widens() {
    let out = compile_and_run(
        r#"<?php
function f(float $x): float { return $x + 0.25; }
echo f(true), " ", f(false);
"#,
    );
    assert_eq!(out, "1.25 0.25");
}

/// Loose equality between a float and an int compiles and compares numerically
/// (`1.5 == 1` is false, `1.0 == 1` is true) — previously an unsupported backend
/// feature. Operands come from runtime-typed locals so the compare survives folding.
#[test]
fn test_float_int_loose_equality() {
    let out = compile_and_run(
        r#"<?php
function eq(float $a, int $b): string { return ($a == $b) ? "1" : "0"; }
echo eq(1.5, 1), eq(1.0, 1), eq(2.0, 2), eq(2.5, 2);
"#,
    );
    assert_eq!(out, "0110");
}

/// A `switch` over a typed `float` subject matches by PHP loose equality, so a
/// fractional subject does not truncate into an integer case label.
#[test]
fn test_float_switch_matches_numeric_case() {
    let out = compile_and_run(
        r#"<?php
function classify(float $x): string {
    switch ($x) {
        case 1:   return "int-one";
        case 1.5: return "onefive";
        case 2.0: return "two";
        default:  return "other";
    }
}
echo classify(1.5), "|", classify(1.0), "|", classify(2.0), "|", classify(3.7);
"#,
    );
    assert_eq!(out, "onefive|int-one|two|other");
}

/// A `switch` over an integer subject still matches a fractional case only when
/// numerically equal (`2` matches `case 2.0` but not `case 2.5`).
#[test]
fn test_int_switch_with_float_case_labels() {
    let out = compile_and_run(
        r#"<?php
function pick(int $n): string {
    switch ($n) {
        case 2.5: return "twofive";
        case 2.0: return "two";
        default:  return "none";
    }
}
echo pick(2), "|", pick(3);
"#,
    );
    assert_eq!(out, "two|none");
}
