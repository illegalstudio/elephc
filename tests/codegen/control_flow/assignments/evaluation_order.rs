//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow, assignments evaluation order, including assignment expression effectful index evaluates once, assignment expression uses rhs mutated variable index, and compound assignment expression uses rhs mutated variable index.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

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

