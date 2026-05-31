//! Purpose:
//! `yield from` delegation: int-array-literal expansion, runtime delegation through an inner Generator value (function call result, local variable, argument passthrough).
//!
//! Called from:
//!  - `cargo test` via the integration test harness; aggregated under
//!    `tests::codegen::generators` in `tests/codegen/generators/mod.rs`.
//!
//! Key details:
//!  - Heap-debug regressions cover ownership of inner generators produced by
//!    direct `yield from <call>` delegation.

use crate::support::*;

/// Verifies `yield from <int_array_literal>` desugars to one Yield node per
/// element at compile time, each carrying its own state index.
#[test]
fn test_generator_yield_from_int_array_literal() {
    let out = compile_and_run(
        r#"<?php
function delegate() {
    yield 0;
    yield from [10, 20, 30];
    yield 99;
}
foreach (delegate() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0 10 20 30 99 ");
}

/// Verifies `yield from $local` where the local holds a Generator pointer
/// returned from another generator function call. Tests delegation through
/// a local variable holding the inner generator.
#[test]
fn test_generator_yield_from_local_generator_variable() {
    // $g holds a Generator pointer returned from inner().
    let out = compile_and_run(
        r#"<?php
function inner() { yield 1; yield 2; yield 3; }
function outer() {
    $g = inner();
    yield from $g;
}
foreach (outer() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "1 2 3 ");
}

/// Verifies runtime delegation via the GeneratorFrame's `delegated_iter` slot.
/// Outer yields 0, hands off to inner which yields 1/2/3, then yields 99
/// once inner is exhausted.
#[test]
fn test_generator_yield_from_inner_generator() {
    let out = compile_and_run(
        r#"<?php
function inner() { yield 1; yield 2; yield 3; }
function outer() {
    yield 0;
    yield from inner();
    yield 99;
}
foreach (outer() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0 1 2 3 99 ");
}

/// Verifies the return value of `yield from` can be captured into a local
/// variable and subsequently yielded. Inner generator returns 42 after yielding 1.
#[test]
fn test_generator_yield_from_return_value_can_be_captured_and_yielded() {
    let out = compile_and_run(
        r#"<?php
function inner() {
    yield 1;
    return 42;
}
function outer() {
    $ret = yield from inner();
    yield $ret;
}
foreach (outer() as $v) {
    echo $v;
    echo "\n";
}
"#,
    );
    assert_eq!(out, "1\n42\n");
}

/// Verifies the delegated return value remains available when the outer
/// generator uses it in normal code after the `yield from` expression.
#[test]
fn test_generator_yield_from_return_value_can_be_echoed_after_delegation() {
    let out = compile_and_run(
        r#"<?php
function inner() {
    yield 1;
    return 7;
}

function outer() {
    $x = yield from inner();
    echo $x;
}

foreach (outer() as $v) {
    echo $v;
}
"#,
    );
    assert_eq!(out, "17");
}

/// Regression test for delegated generator sends: a typed generator may
/// `return` its terminal value, and `send()` on the outer generator must
/// deliver the payload into the currently active inner generator.
#[test]
fn test_generator_yield_from_typed_delegate_forwards_send_payload_and_return() {
    let out = compile_and_run(
        r#"<?php
function inner(): Generator {
    $v = yield "a";
    yield $v;
    return "done";
}

function outer(): Generator {
    $r = yield from inner();
    yield $r;
}

$g = outer();
echo $g->current() . "\n";
echo $g->send("b") . "\n";
$g->next();
echo $g->current() . "\n";
"#,
    );
    assert_eq!(out, "a\nb\ndone\n");
}

/// Verifies a delegated generator return can be consumed by a normal
/// generator-body diagnostic statement after `yield from` completes, while
/// the outer generator still has a null return value when it does not return.
#[test]
fn test_generator_yield_from_return_value_survives_var_dump_statement() {
    let out = compile_and_run(
        r#"<?php
function a() {
    yield 1;
    return 9;
}

function b() {
    $r = yield from a();
    var_dump($r);
}

$g = b();
foreach ($g as $v) {
    echo $v, "\n";
}
var_dump($g->getReturn());
"#,
    );
    assert_eq!(out, "1\nint(9)\nNULL\n");
}

/// Verifies `return yield from inner()` propagates the inner generator's
/// return value (42) so that `$g->getReturn()` returns it correctly.
#[test]
fn test_generator_return_yield_from_delegates_and_returns_inner_value() {
    let out = compile_and_run(
        r#"<?php
function inner() {
    yield 1;
    return 42;
}
function outer() {
    return yield from inner();
}
$g = outer();
foreach ($g as $v) {
    echo $v;
    echo "\n";
}
echo "ret=";
echo $g->getReturn();
echo "\n";
"#,
    );
    assert_eq!(out, "1\nret=42\n");
}

/// Regression test: verifies that an inner generator produced by direct
/// `yield from <call>` delegation is released after completion. Compares
/// heap-debug live counts between a baseline foreach and a delegated foreach;
/// both must report identical leak-free counts.
///
/// Requires `compile_and_run_with_heap_debug` which is only available in
/// full test runs (`cargo test -- --include-ignored`).
#[test]
fn test_generator_yield_from_call_releases_inner_generator_after_completion() {
    let baseline = compile_and_run_with_heap_debug(
        r#"<?php
function inner() { yield 1; yield 2; }
foreach (inner() as $v) { echo $v; echo " "; }
"#,
    );
    assert!(baseline.success, "baseline failed: {}", baseline.stderr);
    assert_eq!(baseline.stdout, "1 2 ");

    let delegated = compile_and_run_with_heap_debug(
        r#"<?php
function inner() { yield 1; yield 2; }
function outer() {
    yield from inner();
}
foreach (outer() as $v) { echo $v; echo " "; }
"#,
    );
    assert!(delegated.success, "program failed: {}", delegated.stderr);
    assert_eq!(delegated.stdout, "1 2 ");
    assert_eq!(
        heap_debug_live_counts(&delegated.stderr),
        heap_debug_live_counts(&baseline.stderr),
        "delegated stderr:\n{}\n\nbaseline stderr:\n{}",
        delegated.stderr,
        baseline.stderr
    );
}

/// Parses `HEAP DEBUG: leak summary:` stderr output and returns
/// (live_blocks, live_bytes). Returns (0, 0) when the summary ends in "clean".
/// Panics if the expected keys are absent from the line.
fn heap_debug_live_counts(stderr: &str) -> (u64, u64) {
    let line = stderr
        .lines()
        .find(|line| line.starts_with("HEAP DEBUG: leak summary:"))
        .unwrap_or_else(|| panic!("missing heap-debug leak summary: {stderr}"));
    if line.ends_with("clean") {
        return (0, 0);
    }
    let live_blocks = line
        .split("live_blocks=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("missing live_blocks in heap-debug line: {line}"));
    let live_bytes = line
        .split("live_bytes=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("missing live_bytes in heap-debug line: {line}"));
    (live_blocks, live_bytes)
}

/// Verifies `yield from` accepts `FROM` (uppercase) as the keyword,
/// testing PHP's case-insensitive builtin keyword handling.
#[test]
fn test_generator_yield_from_case_insensitive_from_keyword() {
    let out = compile_and_run(
        r#"<?php
function inner() { yield 1; yield 2; }
function outer() {
    yield 0;
    yield FROM inner();
}
foreach (outer() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0 1 2 ");
}

/// Verifies `yield from` passes arguments through to the inner generator,
/// then chains a second `yield from` for additional delegation. Tests
/// sequential delegation with argument passthrough.
#[test]
fn test_generator_yield_from_with_arg_passing() {
    let out = compile_and_run(
        r#"<?php
function range_gen(int $start, int $end) {
    $i = $start;
    while ($i < $end) {
        yield $i;
        $i++;
    }
}
function combined() {
    yield from range_gen(0, 3);
    yield from range_gen(10, 12);
}
foreach (combined() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0 1 2 10 11 ");
}
