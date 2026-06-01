//! Purpose:
//! `Generator::send` (int and Mixed payloads routed through the resume sent_value slot) and `Generator::throw` (exception injection into the caller catch).
//!
//! Called from:
//!  - `cargo test` via the integration test harness; aggregated under
//!    `tests::codegen::generators` in `tests/codegen/generators/mod.rs`.
//!
//! Key details:
//!  - Exercises the frame `sent_value` ownership transfer and the runtime
//!    throw path that terminates the generator before unwinding.

use crate::support::*;

/// Verifies `Generator::send(int)` routes the payload into a `YieldAssign` resume,
/// which unboxes the `sent_value` slot back to `int` and assigns it to the LHS local.
/// PHP: `send(100)` makes `$a = yield 1` resolve to `$a = 100` on resume.
#[test]
fn test_generator_send_int_arg_routes_into_yield_assign() {
    // `Generator::send($v)` stashes the boxed Mixed pointer in the
    // sent_value slot; the YieldAssign resume path unboxes it back to an
    // int and stores it into the assignment LHS local. Subsequent
    // `current()` reflects whatever the generator yields after that.
    let out = compile_and_run(
        r#"<?php
function echoer() {
    $a = yield 1;
    $b = yield $a;
    yield $b;
}
$g = echoer();
$g->rewind();
echo $g->current(); echo " ";
$g->send(100);
echo $g->current(); echo " ";
$g->send(200);
echo $g->current();
"#,
    );
    assert_eq!(out, "1 100 200");
}

/// Verifies bare `yield` (no expression) consumes `send()` value into the assignment
/// LHS local without requiring an explicit `yield <expr>` on the right side.
/// PHP: `yield` with no value still writes the sent value to `$x`.
#[test]
fn test_generator_bare_yield_assignment_consumes_send_value() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    $x = yield;
    yield $x;
}
$g = gen();
$g->rewind();
$g->send(7);
echo $g->current();
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies the `sent_value` slot is cleared after a plain `next()` resume that does
/// not assign the yield. A subsequent `current()` sees PHP null, which echoes as an
/// empty string.
#[test]
fn test_generator_send_value_is_cleared_after_plain_resume() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    yield 1;
    yield 2;
    $x = yield 3;
    yield $x;
}
$g = gen();
$g->rewind();
$g->send(99);
$g->next();
$g->next();
echo $g->current();
"#,
    );
    assert_eq!(out, "");
}

/// Verifies `Generator::throw($exc)` sets TERMINATED, publishes the exception in the
/// global slot, and tail-calls the unwinder so the caller's `catch` block receives it.
/// PHP: `throw` terminates the generator before unwinding and the exception propagates
/// to the enclosing `try/catch` in the caller.
#[test]
fn test_generator_throw_propagates_to_caller_catch() {
    // `Generator::throw($exc)` sets TERMINATED, publishes the exception
    // in the global slot, and tail-calls the unwinder; the catch in the
    // caller picks it up.
    let out = compile_and_run(
        r#"<?php
function gen() {
    yield 1;
    yield 2;
}
try {
    $g = gen();
    $g->rewind();
    echo $g->current();
    echo " ";
    $g->throw(new Exception("boom"));
    echo "unreachable";
} catch (Exception $e) {
    echo "caught: ";
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(out, "1 caught: boom");
}

/// Verifies `Generator::send(string)` lands in a Mixed-typed local via refcount transfer
/// (no unboxing to int). The generator alternates `yield <prompt>` / `yield $reply` and
/// the sent string value propagates through the assignment chain.
#[test]
fn test_generator_send_with_string_payload_into_mixed_slot() {
    // `Generator::send($v)` with a string payload now lands in a
    // Mixed-typed local slot via refcount transfer (no unboxing). The
    // generator alternates between `yield <prompt>` and `yield $reply`.
    let out = compile_and_run(
        r#"<?php
function gen() {
    $x = "init";
    $x = yield "first";
    yield $x;
    $x = yield "second";
    yield $x;
}
$g = gen();
$g->rewind();
echo $g->current(); echo " ";
$g->send("alpha");
echo $g->current(); echo " ";
$g->send("beta");
echo $g->current(); echo " ";
$g->send("gamma");
echo $g->current();
"#,
    );
    assert_eq!(out, "first alpha second gamma");
}

/// Verifies `Generator::send(string)` delivers the payload into a fresh
/// `$x = yield ...` assignment and returns the next yielded value to the caller.
#[test]
fn test_generator_send_string_payload_to_fresh_yield_assignment_returns_next_yield() {
    let out = compile_and_run(
        r#"<?php
function g() {
    $x = yield 1;
    echo $x, "\n";
    yield 2;
}

$g = g();
var_dump($g->current());
var_dump($g->send("p"));
"#,
    );
    assert_eq!(out, "int(1)\np\nint(2)\n");
}

/// Regression for issue #308: a value sent into a plain yield assignment
/// remains visible to following generator statements even when the next
/// statement is a ternary echo and the generator then terminates.
#[test]
fn test_generator_send_value_reaches_ternary_echo_before_termination() {
    let out = compile_and_run(
        r#"<?php
function g() {
    $x = yield 1;
    echo $x === null ? "n" : $x;
}

$g = g();
echo $g->current();
$g->send(5);
"#,
    );
    assert_eq!(out, "15");
}

/// Verifies `send()` value participates in Mixed arithmetic when used in expressions
/// on the right side of `yield`. Tests `$a * 2` and `$a + $b` with sent int payloads,
/// and that `send()` on an already-terminated generator returns `null`.
#[test]
fn test_generator_send_value_supports_mixed_arithmetic() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    $a = yield 10;
    $b = yield $a * 2;
    return $a + $b;
}

$g = gen();
echo $g->current();
echo "|";
echo $g->send(5);
echo "|";
echo is_null($g->send(7)) ? "null" : "not-null";
echo "|";
echo $g->getReturn();
"#,
    );
    assert_eq!(out, "10|10|null|12");
}
