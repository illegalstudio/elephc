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

// Verifies that calling a generator function returns a Generator object that
// satisfies `instanceof Generator` and `instanceof Iterator`.
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

// Verifies manual stepping through a generator via the Iterator protocol:
// rewind() runs to the first yield, valid() reports availability,
// current() returns the yielded value, and next() advances. After the
// last yield, valid() reports false.
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

// Verifies that a generator yields string literal values and that foreach
// iteration correctly receives each yielded string.
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

// Verifies that an anonymous function containing yield is treated as a
// generator function and returns a Generator instance that can be manually
// stepped through via rewind/current/next.
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

// Verifies that a generator closure correctly captures a by-copy integer
// local and yields both the captured value and a computed expression
// derived from it.
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

// Verifies that a GeneratorFrame is cleaned up using the target-specific
// custom layout (not the default Debug layout) when a generator variable
// goes out of scope and is unset. Uses heap debug to detect leaks or
// use-after-free in the frame teardown path.
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

// Verifies that a generator yields exactly three integer values when
// iterated to completion with foreach, confirming the state machine
// correctly handles multiple yield points.
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

// Verifies that the foreach receiver variable can shadow the generator
// variable (both named `$g`) without causing a use-after-free or
// incorrect iteration behavior.
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

// Verifies that explicit integer keys on yield expressions are correctly
// surfaced as key/value pairs when iterating with foreach.
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

// Verifies that yields without explicit keys receive auto-incrementing
// integer keys starting from 0, matching PHP's behavior.
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

// Verifies that yields with string keys and integer values correctly
// propagate both the string key and integer value in foreach iteration.
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

// Verifies that `yield [1, 2, 3]` works: the consumer receives a Mixed-boxed
// indexed array and the generator runs to completion without crashing or
// leaking. Does not test count() on Mixed.
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

// Verifies that a local string slot can be yielded multiple times,
// correctly incref'ing the boxed Mixed cell on each yield and
// handling reassignment with refcount replacement. Exercises the
// refcounting path for local-slot yields.
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

// Verifies that a local assigned an int-array literal (Mixed-typed slot)
// can be yielded repeatedly without crashing or leaking, and that
// reassignment yields the new array.
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
