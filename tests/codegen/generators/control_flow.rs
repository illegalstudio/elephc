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
