//! Purpose:
//! Basic generator instantiation, iteration through the Iterator protocol, simple value/key yields, and direct local-slot yield variants.
//!
//! Called from:
//!  - `cargo test` via the integration test harness; aggregated under
//!    `tests::codegen::generators` in `tests/codegen/generators/mod.rs`.
//!
//! Key details:
//!  - Exercises the public Iterator surface emitted for the built-in
//!    Generator class before more specialized generator features run.

use crate::support::*;

/// Verifies that calling a generator function returns a Generator object that
/// satisfies `instanceof Generator` and `instanceof Iterator`.
#[test]
fn test_generator_function_returns_generator_instance() {
    // The result of a generator function call is a real Generator object —
    // it satisfies `instanceof Generator` and `instanceof Iterator`.
    let out = compile_and_run(
        r#"<?php
function gen() {
    yield 1;
}
$g = gen();
if ($g instanceof Generator) { echo "G "; }
if ($g instanceof Iterator) { echo "I "; }
echo "done";
"#,
    );
    assert_eq!(out, "G I done");
}

/// Verifies manual stepping through a generator via the Iterator protocol:
/// rewind() runs to the first yield, valid() reports availability,
/// current() returns the yielded value, and next() advances. After the
/// last yield, valid() reports false.
#[test]
fn test_generator_method_calls_step_through_state() {
    // Stepping the generator manually: rewind() runs to the first yield,
    // valid() reports a value is available, current() returns it,
    // next() advances; after the last yield, valid() reports false.
    let out = compile_and_run(
        r#"<?php
function gen() { yield 7; yield 9; }
$g = gen();
$g->rewind();
echo $g->valid() ? "T" : "F";
echo $g->current();
$g->next();
echo $g->valid() ? "T" : "F";
echo $g->current();
$g->next();
echo $g->valid() ? "T" : "F";
"#,
    );
    assert_eq!(out, "T7T9F");
}

/// Verifies that a generator yields string literal values and that foreach
/// iteration correctly receives each yielded string.
#[test]
fn test_generator_yields_string_values() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    yield "alpha";
    yield "beta";
    yield "gamma";
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "alpha beta gamma ");
}

/// Verifies generator yields int literals.
#[test]
fn test_generator_yields_int_literals() {
    // A generator function with `yield <int_literal>` statements produces
    // those values when iterated with foreach. The state-machine codegen
    // emits a wrapper that allocates a GeneratorFrame plus a resume
    // function that drives the body across yield points.
    let out = compile_and_run(
        r#"<?php
function gen() {
    yield 1;
    yield 2;
}
foreach (gen() as $v) {
    echo $v;
}
echo "done";
"#,
    );
    assert_eq!(out, "12done");
}

/// Verifies that an anonymous function containing yield is treated as a
/// generator function and returns a Generator instance that can be manually
/// stepped through via rewind/current/next.
#[test]
fn test_generator_closure_returns_generator_instance() {
    let out = compile_and_run(
        r#"<?php
$f = function() {
    yield 1;
    yield 2;
};
$g = $f();
$g->rewind();
echo $g->current();
$g->next();
echo $g->current();
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies that a generator closure correctly captures a by-copy integer
/// local and yields both the captured value and a computed expression
/// derived from it.
#[test]
fn test_generator_closure_captures_int_local() {
    let out = compile_and_run(
        r#"<?php
$start = 7;
$f = function() use ($start) {
    yield $start;
    yield $start + 1;
};
foreach ($f() as $v) {
    echo $v;
    echo " ";
}
"#,
    );
    assert_eq!(out, "7 8 ");
}

/// Verifies that a GeneratorFrame is cleaned up using the target-specific
/// custom layout (not the default Debug layout) when a generator variable
/// goes out of scope and is unset. Uses heap debug to detect leaks or
/// use-after-free in the frame teardown path.
#[test]
fn test_generator_frame_cleanup_uses_custom_layout() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function gen() {
    yield "held";
}
$g = gen();
$g->rewind();
echo $g->current();
unset($g);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "held");
}

/// Verifies that a generator yields exactly three integer values when
/// iterated to completion with foreach, confirming the state machine
/// correctly handles multiple yield points.
#[test]
fn test_generator_yields_three_values() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    yield 10;
    yield 20;
    yield 30;
}
foreach (gen() as $v) {
    echo $v;
    echo " ";
}
"#,
    );
    assert_eq!(out, "10 20 30 ");
}

/// Verifies that the foreach receiver variable can shadow the generator
/// variable (both named `$g`) without causing a use-after-free or
/// incorrect iteration behavior.
#[test]
fn test_generator_foreach_can_reuse_receiver_variable() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    yield 10;
    yield 20;
}
$g = gen();
foreach ($g as $g) {
    echo $g . ",";
}
"#,
    );
    assert_eq!(out, "10,20,");
}

/// Verifies that explicit integer keys on yield expressions are correctly
/// surfaced as key/value pairs when iterating with foreach.
#[test]
fn test_generator_yields_with_explicit_int_keys() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    yield 100 => 1;
    yield 200 => 2;
    yield 300 => 3;
}
foreach (gen() as $k => $v) {
    echo $k;
    echo ":";
    echo $v;
    echo " ";
}
"#,
    );
    assert_eq!(out, "100:1 200:2 300:3 ");
}

/// Verifies that yields without explicit keys receive auto-incrementing
/// integer keys starting from 0, matching PHP's behavior.
#[test]
fn test_generator_auto_incrementing_keys() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    yield 5;
    yield 6;
    yield 7;
}
foreach (gen() as $k => $v) {
    echo $k;
    echo "=>";
    echo $v;
    echo " ";
}
"#,
    );
    assert_eq!(out, "0=>5 1=>6 2=>7 ");
}

/// Verifies that yields with string keys and integer values correctly
/// propagate both the string key and integer value in foreach iteration.
#[test]
fn test_generator_yields_with_string_keys_and_int_values() {
    let out = compile_and_run(
        r#"<?php
function pairs() {
    yield "a" => 1;
    yield "b" => 2;
}
foreach (pairs() as $k => $v) {
    echo $k;
    echo $v;
}
"#,
    );
    assert_eq!(out, "a1b2");
}

/// Verifies that `yield [1, 2, 3]` works: the consumer receives a Mixed-boxed
/// indexed array and the generator runs to completion without crashing or
/// leaking. Does not test count() on Mixed.
#[test]
fn test_generator_yields_int_array_literal() {
    // `yield [1, 2, 3]` — the consumer receives a Mixed-boxed indexed
    // array. We verify only that the generator runs to completion past
    // the array yield (count() on Mixed is a separate concern).
    let out = compile_and_run(
        r#"<?php
function gen() {
    yield [1, 2, 3];
    yield [10, 20];
}
foreach (gen() as $arr) {
    echo "ok ";
}
"#,
    );
    assert_eq!(out, "ok ok ");
}

/// Verifies that a local string slot can be yielded multiple times,
/// correctly incref'ing the boxed Mixed cell on each yield and
/// handling reassignment with refcount replacement. Exercises the
/// refcounting path for local-slot yields.
#[test]
fn test_generator_yield_string_from_local_slot() {
    // A local assigned a string literal becomes a Mixed-typed slot;
    // yielding the local incref's the boxed cell so both the slot and
    // the outer `last_value` keep refcounts. Re-assigning the slot
    // refcount-replaces the cell.
    let out = compile_and_run(
        r#"<?php
function gen() {
    $a = "first";
    yield $a;
    $a = "second";
    yield $a;
    $a = "third";
    yield $a;
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "first second third ");
}

/// Verifies that a local assigned an int-array literal (Mixed-typed slot)
/// can be yielded repeatedly without crashing or leaking, and that
/// reassignment yields the new array.
#[test]
fn test_generator_yield_int_array_local_slot() {
    // A local assigned an int-array literal becomes Mixed-typed; the
    // generator can yield it without crashing or leaking.
    let out = compile_and_run(
        r#"<?php
function gen() {
    $arr = [1, 2, 3];
    yield $arr;
    $arr = [10, 20];
    yield $arr;
}
foreach (gen() as $v) { echo "got "; }
"#,
    );
    assert_eq!(out, "got got ");
}

// --- Issue #1086: a declared `Generator` return is not the same as holding a `yield` ---------
//
// PHP decides generator-ness by the `yield` TOKEN and nothing else. A function that merely
// FORWARDS someone else's generator declares exactly the same return type while holding no
// token, and is an ordinary function returning that object:
//
//     function inner(): Generator { yield 1; }
//     function factory(): Generator { return inner(); }   // NOT a generator
//
// Treating the declared type as proof compiled `factory` as a coroutine, so `return inner()`
// became the value `getReturn()` hands back rather than the function's result — iterating it
// never terminated, and driving it by hand reported a valid generator with no current value.
//
// Each fixture asserts a FINITE result, because the failure was a hang rather than a wrong
// value; the by-hand form is included so a regression fails fast instead of spinning.

/// The issue's own shape: a factory forwarding another function's generator.
#[test]
fn test_a_function_returning_another_generator_is_not_itself_a_generator() {
    let out = compile_and_run(
        r#"<?php
function inner(): Generator { yield 1; yield 2; }
function factory(): Generator { return inner(); }
$n = 0;
foreach (factory() as $v) { $n = $n + $v; }
echo "factory:", $n, "\n";
"#,
    );
    assert_eq!(out, "factory:3\n");
}

/// The same through a method, and through a local rather than straight from the call.
#[test]
fn test_a_generator_factory_works_through_a_method_and_a_local() {
    let out = compile_and_run(
        r#"<?php
function inner(): Generator { yield 5; }
class C { public function make(): Generator { return inner(); } }
function viaLocal(): Generator { $g = inner(); return $g; }
$n = 0;
foreach ((new C())->make() as $v) { $n = $n + $v; }
foreach (viaLocal() as $v) { $n = $n + $v; }
echo "both:", $n, "\n";
"#,
    );
    assert_eq!(out, "both:10\n");
}

/// Driving a factory's result by hand. This is the shape that fails FAST when the property is
/// lost: the coroutine reported `valid() === true` with an empty `current()`, where the
/// `foreach` fixtures above hang instead.
#[test]
fn test_a_generator_factorys_result_is_the_inner_generator() {
    let out = compile_and_run(
        r#"<?php
function inner(): Generator { yield 7; }
function factory(): Generator { return inner(); }
$it = factory();
echo "byhand:", var_export($it->valid(), true), ",", $it->current(), "\n";
"#,
    );
    assert_eq!(out, "byhand:true,7\n");
}

/// The `iterable` spelling of the same factory. It already worked — `iterable` is not rewritten
/// to `Generator` for a body with no yield, so the declared type never stood in for the token
/// here. It is the control that says the fix did not simply move the problem.
#[test]
fn test_an_iterable_returning_generator_factory_is_unaffected() {
    let out = compile_and_run(
        r#"<?php
function inner(): Generator { yield 4; }
function factory(): iterable { return inner(); }
$n = 0;
foreach (factory() as $v) { $n = $n + $v; }
echo "iterable:", $n, "\n";
"#,
    );
    assert_eq!(out, "iterable:4\n");
}

/// The negative side: a function that DOES hold a yield and declares `: Generator` is still a
/// generator, and its `return` is still what `getReturn()` reports — including when the only
/// yield in the body is statically dead, which is the shape the declared-type proxy existed to
/// rescue before a prune could no longer delete it.
#[test]
fn test_a_declared_generator_that_holds_a_yield_is_still_a_generator() {
    let out = compile_and_run(
        r#"<?php
function live(): Generator { yield 1; return 9; }
function dead(): Generator { if (false) { yield 1; } return 8; }
$a = live();
foreach ($a as $v) { echo $v; }
$b = dead();
foreach ($b as $v) { echo $v; }
echo ":", $a->getReturn(), ",", $b->getReturn(), "\n";
"#,
    );
    assert_eq!(out, "1:9,8\n");
}

/// Reflection already answered this correctly, and still does.
///
/// `ReflectionFunction::isGenerator()` reads `flags.is_generator`, which was ALWAYS set from the
/// token — the disjunct removed from `attach_generator_source_if_needed` tested a field that had
/// by then been rewritten to the coroutine's `Mixed` body return, so it could never fire. That is
/// the shape of the bug: the flag said "not a generator" while `generator_body_return_type` said
/// "generator", and the factory got a coroutine's body return type without the coroutine.
///
/// So this is a CONTROL: it asserts that removing the other reader did not disturb the one that
/// was right, in the only place the property is observable from PHP without running the
/// function.
#[test]
fn test_reflection_reports_a_generator_factory_as_not_a_generator() {
    let out = compile_and_run(
        r#"<?php
function inner(): Generator { yield 1; }
function factory(): Generator { return inner(); }
function real(): Generator { yield 2; }
$a = new ReflectionFunction("factory");
$b = new ReflectionFunction("real");
var_dump($a->isGenerator(), $b->isGenerator());
"#,
    );
    assert_eq!(out, "bool(false)\nbool(true)\n");
}

/// Everything that consumes a generator accepts a factory's result, which is an ordinary
/// `Generator` object and not a coroutine frame of the factory's own.
///
/// A factory no longer emits the three generator symbols (`_fn_<f>`, `__genbody`, `__gencb`), so
/// any consumer reaching for the body by name rather than driving the returned object would
/// break here rather than in the plain `foreach` the other fixtures use.
#[test]
fn test_a_generator_factorys_result_works_with_every_consumer() {
    let out = compile_and_run(
        r#"<?php
function inner(): Generator { yield 1; yield 2; }
function factory(): Generator { return inner(); }
function outer(): Generator { yield from factory(); }
function one(): Generator { yield 4; }
function oneFactory(): Generator { return one(); }
$n = 0;
foreach (outer() as $v) { $n = $n + $v; }
$c = oneFactory(...);
foreach ($c() as $v) { $n = $n + $v; }
echo "consumers:", count(iterator_to_array(factory())), ",", $n, "\n";
"#,
    );
    assert_eq!(out, "consumers:2,7\n");
}
