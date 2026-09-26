//! Purpose:
//! Regression tests for optimizer rewrites that must preserve syntactic generator identity.
//!
//! Called from:
//! - `tests/codegen/optimizer.rs` in the codegen integration test suite.
//!
//! Key details:
//! - Constant pruning and dead-code elimination must not erase a function's last `yield`.
//! - Fixtures cover functions, methods, closures, trait methods, and a live-yield control.

use crate::support::*;

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
// `optimize::generator_bodies::rewrite_preserving_yield` is what holds the property: a prune
// or dead-code pass that would lose a body's last yield is not performed.

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
