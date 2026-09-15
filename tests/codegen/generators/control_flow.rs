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


/// Regression for #673: a generator whose every `yield` is unreachable is still a generator, and
/// iterating it finishes instead of hanging.
///
/// PHP decides "is this a generator" SYNTACTICALLY, before any folding, and so does the checker —
/// it types `g()` as `Generator` from the `yield` it can see. The AST passes run after checking
/// and stop rewriting a block at its first terminator, so an unreachable `yield` was dropped.
/// That left the two classifications disagreeing: the caller still drove a `Generator` while
/// lowering had produced an ordinary function returning boxed null, and the `foreach` spun
/// forever. The binary never exited — this test hanging IS the regression.
///
/// Both of PHP's ways to spell an immediately-complete generator are here, because they are
/// destroyed by different passes: `if (false) { yield 1; }` by constant-branch pruning, and the
/// idiomatic `return; yield;` by the stop-at-terminator rule the propagation pass applies first.
///
/// All four declaration forms are covered — function, instance method, static method, closure —
/// because each has its own body-rewriting site and its own signature path, and the closure's
/// return type is re-derived at lowering time rather than read from the checker.
///
/// The rows that must NOT change are asserted alongside: a live generator still yields, a
/// conditional `yield` still runs when its branch is taken, and `getReturn()` still carries a
/// value returned before an unreachable `yield`.
///
/// Every expectation is the host PHP 8.5.10 output for the same fixture.
#[test]
fn test_generator_with_only_unreachable_yields_completes_immediately() {
    let out = compile_and_run(
        r#"<?php
function folded() { if (false) { yield 1; } return; }
function idiomatic() { return; yield; }
function dead_after_return() { return 7; yield 1; }
function declared(): Generator { if (false) { yield 1; } return; }
function live() { yield 1; yield 2; }
function conditional(bool $on) { if ($on) { yield 1; } return; }

class Box {
    public function folded() { if (false) { yield 1; } return; }
    public static function idiomatic() { return; yield; }
}

$closure = function () { if (false) { yield 1; } return; };
$closure2 = function () { return; yield; };

function drain(string $label, $gen): void {
    $seen = [];
    foreach ($gen as $v) { $seen[] = $v; }
    echo $label, "=[", implode(",", $seen), "] ret=", var_export($gen->getReturn(), true), "\n";
}

drain("folded", folded());
drain("idiomatic", idiomatic());
drain("dead_after_return", dead_after_return());
drain("declared", declared());
drain("method", (new Box())->folded());
drain("static", Box::idiomatic());
drain("closure", $closure());
drain("closure2", $closure2());
drain("live", live());
drain("cond-off", conditional(false));
drain("cond-on", conditional(true));

var_dump(iterator_to_array(folded()));
$manual = idiomatic();
var_dump($manual->valid());
var_dump($manual->current());
"#,
    );
    assert_eq!(
        out,
        concat!(
            "folded=[] ret=NULL\n",
            "idiomatic=[] ret=NULL\n",
            "dead_after_return=[] ret=7\n",
            "declared=[] ret=NULL\n",
            "method=[] ret=NULL\n",
            "static=[] ret=NULL\n",
            "closure=[] ret=NULL\n",
            "closure2=[] ret=NULL\n",
            "live=[1,2] ret=NULL\n",
            "cond-off=[] ret=NULL\n",
            "cond-on=[1] ret=NULL\n",
            "array(0) {\n}\n",
            "bool(false)\n",
            "NULL\n",
        )
    );
}
