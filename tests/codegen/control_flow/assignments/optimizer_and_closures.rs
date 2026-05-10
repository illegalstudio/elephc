//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow, assignments optimizer and closures, including assignment expression right associative codegen, assignment expression target is not constant propagated, and assignment expression clears stale constants inside same expr.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_assignment_expression_right_associative_codegen() {
    let out = compile_and_run("<?php $x = $y = 4; echo $x; echo ':'; echo $y;");
    assert_eq!(out, "4:4");
}

#[test]
fn test_assignment_expression_target_is_not_constant_propagated() {
    let out = compile_and_run("<?php $x = 1; echo ($x = 2); echo ':'; echo $x;");
    assert_eq!(out, "2:2");
}

#[test]
fn test_assignment_expression_clears_stale_constants_inside_same_expr() {
    let out = compile_and_run("<?php $x = 1; echo (($x = 2) + $x);");
    assert_eq!(out, "4");
}

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

#[test]
fn test_ternary_in_assignment() {
    let out = compile_and_run("<?php $a = 10; $b = 20; $max = $a > $b ? $a : $b; echo $max;");
    assert_eq!(out, "20");
}
