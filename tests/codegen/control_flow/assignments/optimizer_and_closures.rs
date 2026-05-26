//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow, assignments optimizer and closures, including assignment expression right associative codegen, assignment expression target is not constant propagated, and assignment expression clears stale constants inside same expr.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

// Tests that right-associative assignment expressions (`$x = $y = 4`) generate correct
// left-to-right codegen: `$y` is assigned 4 first, then `$x` is assigned the result.
#[test]
fn test_assignment_expression_right_associative_codegen() {
    let out = compile_and_run("<?php $x = $y = 4; echo $x; echo ':'; echo $y;");
    assert_eq!(out, "4:4");
}

// Verifies that the left-hand target of an assignment expression is not constant-propagated
// through the RHS. `$x = 1` initialises `$x`; the expression `($x = 2)` evaluates to 2,
// writes 2 into `$x`, so both the expression result and the final `$x` value are 2.
#[test]
fn test_assignment_expression_target_is_not_constant_propagated() {
    let out = compile_and_run("<?php $x = 1; echo ($x = 2); echo ':'; echo $x;");
    assert_eq!(out, "2:2");
}

// Regression test: the optimizer must not reuse a stale constant fold for `$x` after the
// inner `($x = 2)` has already overwritten it. `(($x = 2) + $x)` must read the new value 2
// (not the pre-assignment constant 1) and produce 4, not 3.
#[test]
fn test_assignment_expression_clears_stale_constants_inside_same_expr() {
    let out = compile_and_run("<?php $x = 1; echo (($x = 2) + $x);");
    assert_eq!(out, "4");
}

// Tests that assignment expressions used as array index and value operands within a larger
// assignment are evaluated exactly once each and in source order. `$i` and `$x` must both
// be assigned their respective side-effect values (0 and 7), and `$items[0]` must receive 7.
#[test]
fn test_assignment_expression_in_array_assignment_operands() {
    let out = compile_and_run(
        r#"<?php
$items = [0];
$items[$i = 0] = ($x = 7);
echo $i . ":" . $x . ":" . $items[0];
"#,
    );
    assert_eq!(out, "0:7:7");
}

// Verifies that an assignment expression inside a closure writes to a slot that is properly
// allocated through the closure's frame. The result of `($x = 9)` must be captured and
// printed as 9 after calling the closure.
#[test]
fn test_assignment_expression_inside_closure_gets_closure_local_slot() {
    let out = compile_and_run(
        r#"<?php
$f = function() {
    echo ($x = 9);
};
$f();
"#,
    );
    assert_eq!(out, "9");
}

// Tests that ternary expressions used as the right-hand side of an assignment are compiled
// correctly. `$max = $a > $b ? $a : $b` must evaluate the comparison, select the correct arm
// (20), and assign it to `$max`.
#[test]
fn test_ternary_in_assignment() {
    let out = compile_and_run("<?php $a = 10; $b = 20; $max = $a > $b ? $a : $b; echo $max;");
    assert_eq!(out, "20");
}
