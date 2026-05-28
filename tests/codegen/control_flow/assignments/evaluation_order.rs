//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow, assignments evaluation order, including assignment expression effectful index evaluates once, assignment expression uses rhs mutated variable index, and compound assignment expression uses rhs mutated variable index.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies that in `$items[idx()] = val()`, idx() is called exactly once (not
/// twice: once to read the old value and once to write the new value). Echo
/// output "iv7:7" confirms idx() runs once, val() runs once, and items[1]=7.
#[test]
fn test_array_assignment_expression_effectful_index_evaluates_once() {
    let out = compile_and_run(
        r#"<?php
function idx(): int {
    echo "i";
    return 1;
}
function val(): int {
    echo "v";
    return 7;
}
$items = [0, 0];
echo ($items[idx()] = val());
echo ":" . $items[1];
"#,
    );
    assert_eq!(out, "iv7:7");
}

/// Verifies that in `$items[$i] = ($i = 1)`, the index `$i` is captured before the
/// RHS mutates it. Output "1:10:1:1" confirms items[0] receives RHS value 1, items[1]
/// stays 20, and $i becomes 1. The RHS assignment to $i does not retroactively
/// change which slot was selected for the initial array access.
#[test]
fn test_array_assignment_expression_uses_rhs_mutated_variable_index() {
    let out = compile_and_run(
        r#"<?php
$items = [10, 20];
$i = 0;
echo ($items[$i] = ($i = 1));
echo ":" . $items[0] . ":" . $items[1] . ":" . $i;
"#,
    );
    assert_eq!(out, "1:10:1:1");
}

/// Verifies that in `$items[$i] += ($i = 1)`, the compound assignment uses the
/// index captured before RHS mutation. Output "21:10:21:1" confirms items[0] (at $i=0)
/// receives 10+1=21, items[1] stays 10, $i becomes 1.
#[test]
fn test_array_compound_assignment_expression_uses_rhs_mutated_variable_index() {
    let out = compile_and_run(
        r#"<?php
$items = [10, 20];
$i = 0;
echo ($items[$i] += ($i = 1));
echo ":" . $items[0] . ":" . $items[1] . ":" . $i;
"#,
    );
    assert_eq!(out, "21:10:21:1");
}

/// Verifies that in `$items[$i + 0] = ($i = 1)`, the computed index expression
/// stabilizes to index 0 before the RHS mutates $i. Output "1:1:20:1" confirms
/// items[0] receives the RHS value 1, items[1] stays 20, and $i becomes 1.
#[test]
fn test_array_assignment_expression_stabilizes_computed_index_before_rhs() {
    let out = compile_and_run(
        r#"<?php
$items = [10, 20];
$i = 0;
echo ($items[$i + 0] = ($i = 1));
echo ":" . $items[0] . ":" . $items[1] . ":" . $i;
"#,
    );
    assert_eq!(out, "1:1:20:1");
}

/// Verifies that in `make_box()->value += inc()`, the receiver expression make_box()
/// is called exactly once. Echo output "mr5" confirms make_box() runs once ("m"),
/// inc() runs once ("r"), and the result is 5. This is a property rather than an
/// array index case.
#[test]
fn test_property_assignment_expression_effectful_receiver_evaluates_once() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public $value = 1;
}
function make_box(): Box {
    echo "m";
    return new Box();
}
function inc(): int {
    echo "r";
    return 4;
}
echo (make_box()->value += inc());
"#,
    );
    assert_eq!(out, "mr5");
}

/// Verifies that in `Registry::$items[idx()] += 2`, idx() is called exactly once.
/// Echo output "i5:5" confirms idx() runs once ("i"), items[0] becomes 5, and the
/// result is 5. This is a static property with a function call index.
#[test]
fn test_static_property_array_assignment_expression_effectful_index_evaluates_once() {
    let out = compile_and_run(
        r#"<?php
class Registry {
    public static $items = [3, 4];
}
function idx(): int {
    echo "i";
    return 0;
}
echo (Registry::$items[idx()] += 2);
echo ":" . Registry::$items[0];
"#,
    );
    assert_eq!(out, "i5:5");
}

/// Verifies that in `$items[idx()] ??= fallback()`, idx() is called exactly once
/// and short-circuit works when the index exists. Echo output "i5:5" confirms idx()
/// runs once ("i"), fallback() is not called (no "f"), and items[0] stays 5.
#[test]
fn test_null_coalesce_assignment_expression_effectful_index_short_circuits_once() {
    let out = compile_and_run(
        r#"<?php
function idx(): int {
    echo "i";
    return 0;
}
function fallback(): int {
    echo "f";
    return 9;
}
$items = [5, 2];
echo ($items[idx()] ??= fallback());
echo ":" . $items[0];
"#,
    );
    assert_eq!(out, "i5:5");
}

/// Verifies that in `$items[$i] ??= ($i = 1)` with $i=2, the index $i is captured
/// before the RHS mutates it, and the null-coalesce assigns because items[2] is null.
/// Output "1:10:1:1" confirms items[2] receives RHS value 1, items[0]=10, items[1]=20,
/// and $i becomes 1.
#[test]
fn test_null_coalesce_assignment_expression_uses_rhs_mutated_variable_index() {
    let out = compile_and_run(
        r#"<?php
$items = [10, 20];
$i = 2;
echo ($items[$i] ??= ($i = 1));
echo ":" . $items[0] . ":" . $items[1] . ":" . $i;
"#,
    );
    assert_eq!(out, "1:10:1:1");
}

/// Verifies that in `$items[$i] ??= ($i = 1)` with $i=0 where items[0] is 10 (not null),
/// the RHS is not evaluated and $i stays 0. Output "10:10:20:0" confirms short-circuit:
/// items[0] remains 10, items[1] remains 20, and $i is not mutated by the RHS.
#[test]
fn test_null_coalesce_assignment_expression_short_circuits_rhs_mutated_index() {
    let out = compile_and_run(
        r#"<?php
$items = [10, 20];
$i = 0;
echo ($items[$i] ??= ($i = 1));
echo ":" . $items[0] . ":" . $items[1] . ":" . $i;
"#,
    );
    assert_eq!(out, "10:10:20:0");
}

/// Verifies that in `$items[$i + 0] ??= ($i = 1)` with $i=0 where items[0]=10 (not null),
/// the computed index stabilizes to 0, the short-circuit prevents RHS evaluation, and
/// $i stays 0. Output "10:10:20:0" confirms neither the index expression nor the
/// RHS mutation affects items or $i.
#[test]
fn test_null_coalesce_assignment_expression_stabilizes_computed_index_before_rhs() {
    let out = compile_and_run(
        r#"<?php
$items = [10, 20];
$i = 0;
echo ($items[$i + 0] ??= ($i = 1));
echo ":" . $items[0] . ":" . $items[1] . ":" . $i;
"#,
    );
    assert_eq!(out, "10:10:20:0");
}

/// Verifies that in `$items[idx()] ??= ($i = 1)` with idx() returning 2, the index
/// function is called exactly once ("i"), the null-coalesce assigns because items[2]
/// is null, and $i is mutated to 1 by the RHS. Output "i1:1:1" confirms idx() runs
/// once, fallback is not called, items[2]=1, and $i=1.
#[test]
fn test_null_coalesce_assignment_expression_effectful_index_mutating_rhs_runs_once() {
    let out = compile_and_run(
        r#"<?php
function idx(): int {
    echo "i";
    return 2;
}
$items = [10, 20];
$i = 0;
echo ($items[idx()] ??= ($i = 1));
echo ":" . $items[2] . ":" . $i;
"#,
    );
    assert_eq!(out, "i1:1:1");
}

/// Verifies that in `Registry::$items[$i] ??= ($i = 1)` with $i=2, the static property
/// index $i is captured before the RHS mutates it. Since items[2] is null, the
/// null-coalesce assigns and $i becomes 1. Output "1:1:1" confirms items[1]=1 and $i=1.
#[test]
fn test_static_property_null_coalesce_assignment_expression_rhs_mutated_index() {
    let out = compile_and_run(
        r#"<?php
class Registry {
    public static $items = [10, 20];
}
$i = 2;
echo (Registry::$items[$i] ??= ($i = 1));
echo ":" . Registry::$items[1] . ":" . $i;
"#,
    );
    assert_eq!(out, "1:1:1");
}

