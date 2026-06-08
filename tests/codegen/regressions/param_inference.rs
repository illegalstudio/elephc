//! Purpose:
//! Regression tests for call-site parameter type inference of untyped parameters.
//! Ensures a parameter called with heterogeneous argument types is inferred as
//! `Mixed` (boxed), not collapsed to a single type that mis-tags scalar arguments.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The bug surfaced via PDO `bindValue` with mixed `?`/`:name` placeholders: an
//!   int argument passed to a param that another call site passed a string to was
//!   coerced to a string, so `is_int` returned false and values were corrupted.

use crate::support::*;

/// A free function with an untyped parameter called with both string and int
/// arguments infers the parameter as `Mixed`, so `is_int` on the int argument is
/// true (the int is boxed, not coerced to string).
#[test]
fn test_untyped_param_heterogeneous_calls_infer_mixed() {
    let out = compile_and_run(
        r#"<?php
function tag($a) { return is_int($a) ? "I" : "N"; }
echo tag("x") . tag(1) . tag(2.5) . tag(7);
"#,
    );
    assert_eq!(out, "NINI");
}

/// The same inference applies to instance-method parameters: params called with
/// both int and string are `Mixed`, so `is_int` and the round-tripped argument
/// values are correct regardless of call order.
#[test]
fn test_untyped_method_param_heterogeneous_calls_infer_mixed() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function put($a, $b, int $c) {
        return (is_int($a) ? "I" : "N") . $a . ":" . $b . ":" . $c;
    }
}
$o = new Box();
echo $o->put(1, "x", 5) . "|" . $o->put("y", 2, 6);
"#,
    );
    assert_eq!(out, "I1:x:5|Ny:2:6");
}

/// A parameter that is genuinely homogeneous (only int call sites) stays a
/// concrete int and is not over-widened to `Mixed`.
#[test]
fn test_untyped_param_homogeneous_int_stays_int() {
    let out = compile_and_run(
        r#"<?php
function only_int($a) { return is_int($a) ? "I" : "N"; }
echo only_int(1) . only_int(2) . only_int(3);
"#,
    );
    assert_eq!(out, "III");
}
