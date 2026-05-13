//! Purpose:
//! Generators that yield computed values: arithmetic on parameters and locals, post-increment, division, constant folding, and user-function calls in yield expressions.
//!
//! Called from:
//!  - `cargo test` via the integration test harness; aggregated under
//!    `tests::codegen::generators` in `tests/codegen/generators/mod.rs`.
//!
//! Key details:
//!  - Focuses on values produced inside the generated resume state machine,
//!    including preserved locals and helper-call results.

use crate::support::*;

#[test]
fn test_generator_int_division_in_yield_expr() {
    // `$i / 2 * 2 == $i` is true exactly when $i is even (signed integer
    // division truncates toward zero). The generator emits even numbers.
    let out = compile_and_run(
        r#"<?php
function gen(int $n) {
    for ($i = 0; $i < $n; $i++) {
        if ($i == $i / 2 * 2) {
            yield $i;
        }
    }
}
foreach (gen(10) as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0 2 4 6 8 ");
}

#[test]
fn test_generator_yields_int_parameters() {
    let out = compile_and_run(
        r#"<?php
function gen(int $a, int $b) {
    yield $a;
    yield $b;
    yield $a;
}
foreach (gen(7, 9) as $v) {
    echo $v;
    echo " ";
}
"#,
    );
    assert_eq!(out, "7 9 7 ");
}

#[test]
fn test_generator_yields_const_folded_arithmetic() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    yield 1 + 2;
    yield 3 * 4;
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "3 12 ");
}

#[test]
fn test_generator_yields_param_arithmetic() {
    let out = compile_and_run(
        r#"<?php
function gen(int $a) {
    yield $a + 1;
    yield $a * 2;
}
foreach (gen(10) as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "11 20 ");
}

#[test]
fn test_generator_local_variable_across_yields() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    $x = 5;
    yield $x;
    $x = 10;
    yield $x;
    $x = 99;
    yield $x;
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "5 10 99 ");
}

#[test]
fn test_generator_counter_with_arithmetic_assignment() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    $i = 0;
    yield $i;
    $i = $i + 1;
    yield $i;
    $i = $i + 1;
    yield $i;
    $i = $i + 1;
    yield $i;
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0 1 2 3 ");
}

#[test]
fn test_generator_post_increment_local() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    $i = 10;
    yield $i;
    $i++;
    yield $i;
    $i++;
    yield $i;
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "10 11 12 ");
}

#[test]
fn test_generator_calls_user_function() {
    // `yield helper($i)` evaluates the user function call into x0 then
    // boxes the result. v1 supports up to 8 int arguments.
    let out = compile_and_run(
        r#"<?php
function helper(int $x): int { return $x + 100; }
function gen() {
    yield helper(1);
    yield helper(2);
    yield helper(3);
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "101 102 103 ");
}

#[test]
fn test_generator_calls_user_function_with_stack_passed_arg() {
    let out = compile_and_run(
        r#"<?php
function sum7(int $a, int $b, int $c, int $d, int $e, int $f, int $g): int {
    return $a + $b + $c + $d + $e + $f + $g;
}
function gen() {
    yield sum7(1, 2, 3, 4, 5, 6, 7);
}
foreach (gen() as $v) {
    echo $v;
}
"#,
    );
    assert_eq!(out, "28");
}

#[test]
fn test_generator_stack_passed_parameter_survives_in_frame() {
    let out = compile_and_run(
        r#"<?php
function gen(int $a, int $b, int $c, int $d, int $e, int $f, int $g) {
    yield $g;
    yield $a + $g;
}
foreach (gen(1, 2, 3, 4, 5, 6, 7) as $v) {
    echo $v;
    echo " ";
}
"#,
    );
    assert_eq!(out, "7 8 ");
}

#[test]
fn test_generator_calls_user_function_in_arithmetic() {
    let out = compile_and_run(
        r#"<?php
function dbl(int $x): int { return $x * 2; }
function gen() {
    $i = 1;
    while ($i < 5) {
        yield dbl($i) + 10;
        $i++;
    }
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "12 14 16 18 ");
}

#[test]
fn test_generator_combined_param_key_and_value() {
    let out = compile_and_run(
        r#"<?php
function gen(int $start, int $end) {
    yield $start => 1;
    yield $end => 2;
    yield 99 => $start;
}
foreach (gen(10, 20) as $k => $v) {
    echo $k;
    echo "->";
    echo $v;
    echo " ";
}
"#,
    );
    assert_eq!(out, "10->1 20->2 99->10 ");
}
