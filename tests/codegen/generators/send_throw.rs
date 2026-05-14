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
    assert_eq!(out, "0");
}

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
