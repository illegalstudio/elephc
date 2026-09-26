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

// --- Issue #1085: a dead `yield` still makes the function a generator -----------------------
//
// PHP decides generator-ness SYNTACTICALLY, at declaration: a body holding a `yield` token is a
// generator even when no `yield` can ever run, so each function below returns an EMPTY
// generator. Constant-folding the dead branch away deletes the token, and the function silently
// stops being one: it returns a boxed null where its caller expects a `Generator`, `valid()`
// answers `true` forever, and `foreach` never terminates. Every fixture here therefore asserts a
// FINITE result rather than only a value — a regression shows up as a hung test, so the
// `valid()` forms are included alongside the `foreach` ones to fail fast instead of hanging.
//
// `optimize::generator_bodies::rewrite_preserving_yield` is what holds the property: a rewrite
// that would lose a body's last yield is not performed.

/// A `yield` inside `while (false)` is dead, and the function is still a generator.
#[test]
fn test_while_false_yield_is_still_a_generator() {
    let out = compile_and_run(
        r#"<?php
function g(): iterable { while (false) { yield 1; } }
$n = 0;
foreach (g() as $v) { $n = $n + $v; }
echo "while:", $n, ",", count(iterator_to_array(g())), "\n";
"#,
    );
    assert_eq!(out, "while:0,0\n");
}

/// The `if (false)` form, which reaches the same fold through a different statement.
#[test]
fn test_if_false_yield_is_still_a_generator() {
    let out = compile_and_run(
        r#"<?php
function g(): iterable { if (false) { yield 1; } }
$n = 0;
foreach (g() as $v) { $n = $n + $v; }
echo "if:", $n, ",", count(iterator_to_array(g())), "\n";
"#,
    );
    assert_eq!(out, "if:0,0\n");
}

/// A `yield` in a `switch` arm the subject never selects. The subject is a parameter, so this
/// yield is unreachable at RUN time rather than statically dead — the control that says the two
/// are not the same case.
#[test]
fn test_dead_switch_arm_yield_is_still_a_generator() {
    let out = compile_and_run(
        r#"<?php
function g(int $k): iterable { switch ($k) { case 1: yield 1; break; default: break; } }
$n = 0;
foreach (g(2) as $v) { $n = $n + $v; }
echo "switch:", $n, ",", count(iterator_to_array(g(2))), "\n";
"#,
    );
    assert_eq!(out, "switch:0,0\n");
}

/// A `yield` after an unconditional `throw` is unreachable, and the throw must still propagate
/// out of the generator rather than being swallowed with the body.
#[test]
fn test_yield_after_an_unconditional_throw_still_throws() {
    let out = compile_and_run(
        r#"<?php
function g(): iterable { throw new RuntimeException("x"); yield 1; }
try { foreach (g() as $v) { echo $v; } } catch (RuntimeException $e) { echo "throw:", $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(out, "throw:x\n");
}

/// A dead `yield from` is the delegating form of the same question.
#[test]
fn test_dead_yield_from_is_still_a_generator() {
    let out = compile_and_run(
        r#"<?php
function inner(): iterable { yield 1; }
function g(): iterable { if (false) { yield from inner(); } }
$n = 0;
foreach (g() as $v) { $n = $n + $v; }
echo "yieldfrom:", $n, ",", count(iterator_to_array(g())), "\n";
"#,
    );
    assert_eq!(out, "yieldfrom:0,0\n");
}

/// The same body reached through a TRAIT method, where the declaration the checker sees and the
/// one the class ends up with are not the same syntax node.
#[test]
fn test_dead_yield_in_a_trait_method_is_still_a_generator() {
    let out = compile_and_run(
        r#"<?php
trait T { public function g(): iterable { while (false) { yield 1; } } }
class C { use T; }
$o = new C();
$n = 0;
foreach ($o->g() as $v) { $n = $n + $v; }
echo "trait:", $n, ",", count(iterator_to_array($o->g())), "\n";
"#,
    );
    assert_eq!(out, "trait:0,0\n");
}

/// A CLOSURE with a dead yield. Closures prune their body through a different entry point than
/// named functions and methods, and an `iterable` return type is not rewritten to `Generator`
/// for them, so this shape has no second signal to fall back on: before the fix it was refused
/// outright with `runtime_call with 0 operands returning PHP type Iterable`.
///
/// Deliberately no `iterator_to_array()` here, unlike its siblings: that builtin over a
/// closure-returned generator is refused with `iterator_to_array for PHP type Mixed`, which is
/// a separate gap and would assert it rather than this.
#[test]
fn test_dead_yield_in_a_closure_is_still_a_generator() {
    let out = compile_and_run(
        r#"<?php
$g = function (): iterable { while (false) { yield 1; } };
$n = 0;
foreach ($g() as $v) { $n = $n + $v; }
$it = $g();
echo "closure:", $n, ",", var_export($it->valid(), true), "\n";
"#,
    );
    assert_eq!(out, "closure:0,false\n");
}

/// Driving the iterator by hand: an empty generator reports `valid() === false` and a `null`
/// current, rather than reporting a value it never yielded. This is the shape that fails FAST
/// when the property is lost, where the `foreach` fixtures above hang instead.
#[test]
fn test_an_empty_generator_reports_invalid_by_hand() {
    let out = compile_and_run(
        r#"<?php
function g(): iterable { while (false) { yield 1; } }
$it = g();
echo "valid:", var_export($it->valid(), true), ",", var_export($it->current(), true), "\n";
"#,
    );
    assert_eq!(out, "valid:false,NULL\n");
}

/// A dead yield does not disturb `getReturn()`: the body still runs to its `return`, and the
/// value is readable once the empty generator has finished.
#[test]
fn test_an_empty_generator_still_carries_its_return_value() {
    let out = compile_and_run(
        r#"<?php
function g(): Generator { if (false) { yield 1; } return 9; }
$it = g();
foreach ($it as $v) { echo $v; }
echo ":", $it->getReturn(), "\n";
"#,
    );
    assert_eq!(out, ":9\n");
}

/// A branch that is dead by PATH rather than by a literal: no constant appears anywhere, so the
/// pruning pass cannot fold it at all. This one already worked — it is here as the boundary
/// marker for the dead-code pass's path-sensitive folding, which the guard now also covers, so
/// a later change that starts folding this shape cannot reopen the bug silently.
#[test]
fn test_a_path_dead_yield_survives_dead_code_elimination() {
    let out = compile_and_run(
        r#"<?php
function g(bool $c): iterable {
    if ($c) {
        if (!$c) { yield 1; }
    }
}
$it = g(true);
echo "path:", var_export($it->valid(), true), "\n";
"#,
    );
    assert_eq!(out, "path:false\n");
}

/// An `elseif` chain whose LEADING condition is the constant one. The chain is rewritten as a
/// whole rather than arm by arm, so it survives the pruning pass — guard engaged, body
/// restored — and is collapsed by the dead-code pass instead. This is the fixture that pins the
/// second boundary: it is the one that still failed with the pruning guard alone.
#[test]
fn test_dead_yield_in_a_constant_elseif_chain_is_still_a_generator() {
    let out = compile_and_run(
        r#"<?php
function g(): iterable { if (false) { echo ""; } elseif (false) { yield 1; } }
$it = g();
echo "elseif:", var_export($it->valid(), true), "\n";
"#,
    );
    assert_eq!(out, "elseif:false\n");
}

/// The control for all of the above: a LIVE `while` yield still yields, so the guard that keeps
/// dead branches has not simply disabled the fold for every generator.
#[test]
fn test_a_live_while_yield_is_unaffected() {
    let out = compile_and_run(
        r#"<?php
function g(): iterable { $i = 0; while ($i < 3) { yield $i; $i = $i + 1; } }
$n = 0;
foreach (g() as $v) { $n = $n + $v; }
echo "live:", $n, "\n";
"#,
    );
    assert_eq!(out, "live:3\n");
}


/// A FACTORY declaring `: Generator` is not a generator function.
///
/// PHP decides generator-ness syntactically: a body containing `yield` is a generator, and a
/// body that merely declares and RETURNS a `Generator` is an ordinary function. Inferring it
/// from `return_type == Generator` instead conflates the two — the factory's public symbol
/// became a generator constructor, its `return inner()` lowered to `Generator::getReturn()`,
/// and the `foreach` over it saw nothing. Raised in review on the #673 fix (issue #1086).
///
/// The three shapes here are the ones that must disagree with each other: a factory with no
/// yield at all, a factory whose returned generator is built inline, and a real generator that
/// also declares `: Generator` and must stay one.
#[test]
fn test_generator_factory_is_not_itself_a_generator() {
    let out = compile_and_run(
        r#"<?php
function inner() { yield 1; yield 2; }
function factory(): Generator { return inner(); }
function declaredGenerator(): Generator { yield 8; yield 9; }

class Build {
    public function make(): Generator { return inner(); }
    public static function makeStatic(): Generator { return inner(); }
}

$f = "";
foreach (factory() as $v) { $f .= $v . ","; }
$d = "";
foreach (declaredGenerator() as $v) { $d .= $v . ","; }
$m = "";
$b = new Build();
foreach ($b->make() as $v) { $m .= $v . ","; }
$s = "";
foreach (Build::makeStatic() as $v) { $s .= $v . ","; }
$c = "";
$closure = function (): Generator { return inner(); };
foreach ($closure() as $v) { $c .= $v . ","; }
echo "factory=", $f, " declared=", $d, " method=", $m, " static=", $s, " closure=", $c;
"#,
    );
    assert_eq!(
        out,
        "factory=1,2, declared=8,9, method=1,2, static=1,2, closure=1,2,"
    );
}

/// The remaining ways to spell a generator whose every `yield` is dead.
///
/// Raised as #1085 after the #673 fix: the original fixture covered `if (false)`, `return;
/// yield;` and `return 7; yield`, leaving `while (false)`, a dead `switch` arm, a dead
/// `yield from` and trait methods untested. They are the same mechanism rather than new logic,
/// but each reaches it through a different pass, so each is a place the classification could
/// have been lost.
///
/// All of them are generators that complete immediately: PHP iterates nothing and `getReturn()`
/// answers whatever the body returned. `traitLive` is the control — a trait method that really
/// yields must still yield.
#[test]
fn test_every_dead_yield_shape_stays_a_generator() {
    let out = compile_and_run(
        r#"<?php
function whileFalse() { while (false) { yield 1; } return; }
function deadSwitch() { switch (0) { case 1: yield 1; break; } return; }
function deadYieldFrom() { if (false) { yield from [1, 2]; } return; }
function deadWithReturnValue() { if (false) { yield 1; } return 7; }

trait Yielder {
    public function traitDead() { if (false) { yield 1; } return; }
    public function traitLive() { yield 5; }
}
class Holder { use Yielder; }

function show(string $label, $gen) {
    $vals = [];
    foreach ($gen as $v) { $vals[] = $v; }
    echo $label, "=[", implode(",", $vals), "] ret=";
    var_dump($gen->getReturn());
}

show("whileFalse", whileFalse());
show("deadSwitch", deadSwitch());
show("deadYieldFrom", deadYieldFrom());
show("deadWithReturnValue", deadWithReturnValue());
$h = new Holder();
show("traitDead", $h->traitDead());
show("traitLive", $h->traitLive());
"#,
    );
    assert_eq!(
        out,
        concat!(
            "whileFalse=[] ret=NULL\n",
            "deadSwitch=[] ret=NULL\n",
            "deadYieldFrom=[] ret=NULL\n",
            "deadWithReturnValue=[] ret=int(7)\n",
            "traitDead=[] ret=NULL\n",
            "traitLive=[5] ret=NULL\n",
        )
    );
}
