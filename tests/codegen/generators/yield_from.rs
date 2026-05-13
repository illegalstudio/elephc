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

#[test]
fn test_generator_yield_from_int_array_literal() {
    // `yield from <int_array_literal>` desugars to one Yield node per
    // element at compile time, each carrying its own state index.
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

#[test]
fn test_generator_yield_from_local_generator_variable() {
    // `yield from $local` where the local holds a Generator pointer
    // (returned from another generator function call).
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

#[test]
fn test_generator_yield_from_inner_generator() {
    // Runtime delegation via the GeneratorFrame's `delegated_iter` slot:
    // outer yields 0, hands off to inner which yields 1/2/3, then yields
    // 99 once inner is exhausted.
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
