//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of type-related builtins division, including integer division returns float, integer division exact, and division assign updates type.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies `/` produces a float-formatted string (PHP semantics: non-integer division returns float).
#[test]
fn test_int_division_returns_float() {
    let out = compile_and_run("<?php echo 10 / 3;");
    assert_eq!(out, "3.3333333333333");
}

/// Verifies exact division still returns float-formatted output, not integer.
#[test]
fn test_int_division_exact() {
    let out = compile_and_run("<?php echo 10 / 2;");
    assert_eq!(out, "5");
}

/// Verifies compound assignment `/=` updates the variable type to float.
#[test]
fn test_division_assign_updates_type() {
    let out = compile_and_run("<?php $x = 10; $x /= 3; echo $x;");
    assert_eq!(out, "3.3333333333333");
}

/// Verifies float arithmetic is used when summing multiple division results.
#[test]
fn test_division_in_expression() {
    let out = compile_and_run("<?php echo 1 / 3 + 1 / 3 + 1 / 3;");
    assert_eq!(out, "1");
}

/// Verifies `intdiv()` returns an integer (truncates toward zero).
#[test]
fn test_intdiv_still_returns_int() {
    let out = compile_and_run("<?php echo intdiv(10, 3);");
    assert_eq!(out, "3");
}

/// Verifies `intdiv()` with exact division returns integer without decimal.
#[test]
fn test_intdiv_exact() {
    let out = compile_and_run("<?php echo intdiv(10, 5);");
    assert_eq!(out, "2");
}

/// Verifies `intdiv()` with negative dividend truncates toward zero (not floor).
#[test]
fn test_intdiv_negative() {
    let out = compile_and_run("<?php echo intdiv(-7, 2);");
    assert_eq!(out, "-3");
}

/// Verifies float division by zero produces `INF`.
#[test]
fn test_division_by_zero_inf() {
    let out = compile_and_run("<?php echo 1.0 / 0.0;");
    assert_eq!(out, "INF");
}

/// Regression: `intdiv()` must unbox a `Mixed` operand before dividing.
///
/// `$secs` is a local derived from an interface-typed method call, so the checker types it
/// `Mixed` (boxed). Before the fix the emitter divided the boxed cell pointer instead of the
/// integer payload, yielding garbage (e.g. 50228 instead of 2), while `%` and `(int)(a/b)`
/// evaluated right next to it were unaffected. The fix routes both `intdiv` operands through the
/// shared `coerce_to_int` helper (the same one `/`, `%`, and comparisons use).
#[test]
fn test_intdiv_unboxes_mixed_method_local_operand() {
    let out = compile_and_run(
        r#"<?php
interface I { public function gv(): int; }
class C implements I {
    public int $v = 0;
    public function gv(): int { return $this->v; }
    public function span(I $o): int { $secs = $o->gv() - $this->v; return intdiv($secs, 86400); }
}
$x = new C(); $x->v = 1700000000;
$y = new C(); $y->v = 1700200000;
echo $x->span($y);
"#,
    );
    assert_eq!(out, "2");
}

/// Regression: both `intdiv()` operands are unboxed when they come from heterogeneous
/// (Mixed-valued) associative arrays, exercising the Mixed divisor path as well as the dividend.
#[test]
fn test_intdiv_unboxes_mixed_array_operands() {
    let out = compile_and_run(
        r#"<?php
$a = ["n" => 200000, "tag" => "x"];
$b = ["d" => 86400, "tag" => "y"];
echo intdiv($a["n"], $b["d"]);
"#,
    );
    assert_eq!(out, "2");
}
