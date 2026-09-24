//! Purpose:
//! Generators with control-flow inside the body: if/elseif/else chains, while/do-while/for loops with break/continue, switch with default branches, and the Fibonacci benchmark.
//!
//! Called from:
//!  - `cargo test` via the integration test harness; aggregated under
//!    `tests::codegen::generators` in `tests/codegen/generators/mod.rs`.
//!
//! Key details:
//!  - Covers resume labels embedded inside structured control flow where
//!    break/continue/switch paths must preserve generator state.

use crate::support::*;

/// Tests switch with default branch inside a generator.
/// Verifies that case 2 branches to "two" and falls through to yield 2, while case 7
/// takes the default branch yielding "other" then 7.
#[test]
fn test_generator_switch_with_default_branch() {
    let out = compile_and_run(
        r#"<?php
function gen(int $n) {
    switch ($n) {
        case 1:
            yield "one";
            break;
        case 2:
            yield "two";
            break;
        default:
            yield "other";
    }
    yield $n;
}
foreach (gen(2) as $v) { echo $v; echo " "; }
echo "| ";
foreach (gen(7) as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "two 2 | other 7 ");
}

/// Tests a while loop inside a generator, yielding values 0 through 4.
#[test]
fn test_generator_with_while_loop() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    $i = 0;
    while ($i < 5) {
        yield $i;
        $i++;
    }
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0 1 2 3 4 ");
}

/// Tests if/else inside a generator, verifying the taken branch is yielded first
/// followed by the parameter value. Covers both >5 and <=5 paths.
#[test]
fn test_generator_with_if_else() {
    let out = compile_and_run(
        r#"<?php
function gen(int $n) {
    if ($n > 5) {
        yield 100;
    } else {
        yield 200;
    }
    yield $n;
}
foreach (gen(10) as $v) { echo $v; echo " "; }
echo "| ";
foreach (gen(3) as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "100 10 | 200 3 ");
}

/// Tests a for loop inside a generator, yielding values 0 through 4.
#[test]
fn test_generator_with_for_loop() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    for ($i = 0; $i < 5; $i++) {
        yield $i;
    }
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0 1 2 3 4 ");
}

/// Tests break inside a for loop within a generator; stops after yielding 0-4.
#[test]
fn test_generator_break_in_for() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    for ($i = 0; $i < 100; $i++) {
        if ($i == 5) { break; }
        yield $i;
    }
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0 1 2 3 4 ");
}

/// Tests that `continue` inside a for loop jumps to the update step, not the loop top.
/// Without correct resume labeling the generator would hang with $i stuck at 3.
#[test]
fn test_generator_continue_in_for_runs_update() {
    // `continue` must jump to the for-loop's update step, NOT the loop top —
    // otherwise $i would never increment past 3 and the generator hangs.
    let out = compile_and_run(
        r#"<?php
function gen() {
    for ($i = 0; $i < 10; $i++) {
        if ($i == 3) { continue; }
        if ($i == 7) { continue; }
        yield $i;
    }
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0 1 2 4 5 6 8 9 ");
}

/// Tests elseif chain inside a generator across four input values:
/// negative (-5), zero, single-digit (7), and large (50).
#[test]
fn test_generator_elseif_chain() {
    let out = compile_and_run(
        r#"<?php
function classify(int $n) {
    if ($n < 0) {
        yield 0 - 1;
    } elseif ($n == 0) {
        yield 0;
    } elseif ($n < 10) {
        yield 1;
    } else {
        yield 100;
    }
}
foreach (classify(0 - 5) as $v) { echo $v; echo " "; }
foreach (classify(0) as $v) { echo $v; echo " "; }
foreach (classify(7) as $v) { echo $v; echo " "; }
foreach (classify(50) as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "-1 0 1 100 ");
}

/// Tests nested for loops with break in the inner loop; yields i*10+j for j=0,1.
#[test]
fn test_generator_nested_for_with_break() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    for ($i = 0; $i < 3; $i++) {
        for ($j = 0; $j < 3; $j++) {
            if ($j == 2) { break; }
            yield $i * 10 + $j;
        }
    }
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0 1 10 11 20 21 ");
}

/// Tests do-while inside a generator; body executes at least once, yielding 0, 1, 2.
#[test]
fn test_generator_do_while() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    $i = 0;
    do {
        yield $i;
        $i++;
    } while ($i < 3);
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0 1 2 ");
}

/// Verifies that a yield inside a try body is accepted and resumes normally.
#[test]
fn test_generator_yield_inside_try_body() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    try {
        yield 1;
    } catch (Exception $e) {
        echo "caught";
    }
}
$g = gen();
echo $g->current();
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies that a yield in a catch body is accepted by generator validation.
#[test]
fn test_generator_yield_inside_catch_body_compiles() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    try {
    } catch (Exception $e) {
        yield 1;
    }
}
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies that a generator runs try-body side effects before yielding from finally.
#[test]
fn test_generator_yield_inside_finally_body() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    try {
        echo "a";
    } finally {
        yield 2;
    }
}
$g = gen();
echo $g->current();
"#,
    );
    assert_eq!(out, "a2");
}

/// Tests the Fibonacci generator as a benchmark for stateful generator loop logic.
/// Produces the first 10 Fibonacci numbers: 0 1 1 2 3 5 8 13 21 34.
#[test]
fn test_generator_fibonacci() {
    let out = compile_and_run(
        r#"<?php
function fib(int $count) {
    $a = 0;
    $b = 1;
    $i = 0;
    while ($i < $count) {
        yield $a;
        $c = $a + $b;
        $a = $b;
        $b = $c;
        $i++;
    }
}
foreach (fib(10) as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0 1 1 2 3 5 8 13 21 34 ");
}
