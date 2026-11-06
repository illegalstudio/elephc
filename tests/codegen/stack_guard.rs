//! Purpose:
//! Integration or regression tests for the call-stack overflow guard: unbounded recursion
//! must end in a controlled fatal on stderr with a non-zero exit instead of a raw SIGSEGV,
//! while legitimately deep recursion must keep working on both the OS stack and the
//! coroutine stacks used by generators and fibers.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries; the failure cases assert on the
//!   stderr text and on the mere fact that the process did not succeed, because a raw
//!   signal death would also be non-zero — the message is what proves the guard fired.
//! - Overflow fixtures do observable work after recursion so tail-call elimination cannot
//!   turn their deliberately unbounded calls into infinite loops.
//! - The "still succeeds" cases are the false-positive gate: they must stay comfortably
//!   under the real limits (roughly 50k frames on a default 8 MiB OS stack and roughly 1k
//!   frames on a 256 KiB fiber stack) so the reserve can never make them flaky.

use crate::support::*;

/// PHP 8.3+ reports runaway recursion as `Maximum call stack size ... reached. Infinite
/// recursion?`; elephc reports the same condition with the same wording minus the byte
/// count and source location, which the fatal path cannot produce.
const OVERFLOW_MESSAGE: &str = "Maximum call stack size reached. Infinite recursion?";

/// Verifies that direct unbounded recursion reports the controlled call-stack fatal on
/// stderr instead of dying from SIGSEGV with no diagnostic.
#[test]
fn test_direct_infinite_recursion_reports_controlled_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function deep(int $n): int { return 1 + deep($n + 1); }
echo deep(0);
"#,
    );
    assert!(err.contains(OVERFLOW_MESSAGE), "{err}");
}

/// Verifies that mutual recursion is caught as well: the guard lives in every prologue, so
/// it does not depend on recognizing a self-call.
#[test]
fn test_mutual_infinite_recursion_reports_controlled_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function ping(int $n): int { return 1 + pong($n + 1); }
function pong(int $n): int { return 1 + ping($n + 1); }
echo ping(0);
"#,
    );
    assert!(err.contains(OVERFLOW_MESSAGE), "{err}");
}

/// Verifies that unbounded recursion through a method reports the same controlled fatal,
/// covering the method prologue path rather than the plain-function one.
#[test]
fn test_infinite_method_recursion_reports_controlled_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Recurse {
    public function go(int $n): int { $x = $this->go($n + 1); return $x; }
}
$r = new Recurse();
echo $r->go(0);
"#,
    );
    assert!(err.contains(OVERFLOW_MESSAGE), "{err}");
}

/// False-positive gate: a linked-list style walk 20 000 frames deep and a recursive
/// Fibonacci must both still run to completion. If the guard's reserve were too large, or
/// the published floor were computed from the wrong stack, this is what would break.
#[test]
fn test_deep_but_legitimate_recursion_still_succeeds() {
    let out = compile_and_run(
        r#"<?php
function walk(int $n): int { if ($n <= 0) { return 0; } return 1 + walk($n - 1); }
function fib(int $n): int { return $n < 2 ? $n : fib($n - 1) + fib($n - 2); }
echo walk(20000), ",", fib(24);
"#,
    );
    assert_eq!(out, "20000,46368");
}

/// False-positive gate for recursion that returns refcounted values, which take the longer
/// prologue path (parameter retain, cleanup-slot zeroing) than the plain integer case.
#[test]
fn test_deep_recursion_over_strings_still_succeeds() {
    let out = compile_and_run(
        r#"<?php
function chain(int $n): string { if ($n <= 0) { return "end"; } return chain($n - 1); }
echo chain(5000);
"#,
    );
    assert_eq!(out, "end");
}

/// Verifies that unbounded recursion inside a generator body reports the controlled fatal.
/// Generator bodies run on a 256 KiB mmap'd coroutine stack, so before the guard existed
/// this path died on the coroutine guard page instead of the OS stack.
#[test]
fn test_infinite_recursion_inside_generator_reports_controlled_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function deep(int $n): int { return 1 + deep($n + 1); }
function gen(): Generator { yield deep(0); }
foreach (gen() as $v) { echo $v; }
"#,
    );
    assert!(err.contains(OVERFLOW_MESSAGE), "{err}");
}

/// Verifies that unbounded recursion inside a Fiber body reports the controlled fatal too,
/// exercising the same coroutine-stack floor through the explicit Fiber API.
#[test]
fn test_infinite_recursion_inside_fiber_reports_controlled_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function deep(int $n): int { return 1 + deep($n + 1); }
$f = new Fiber(function (): void { Fiber::suspend(deep(0)); });
$f->start();
echo "unreachable";
"#,
    );
    assert!(err.contains(OVERFLOW_MESSAGE), "{err}");
}

/// False-positive gate for coroutine stacks: a generator and a fiber that each recurse a
/// few hundred frames deep must still produce their values. This is the case that would
/// break if the fiber floor were left pointing at the OS-thread stack.
#[test]
fn test_bounded_recursion_inside_generator_and_fiber_still_succeeds() {
    let out = compile_and_run(
        r#"<?php
function walk(int $n): int { if ($n <= 0) { return 0; } return 1 + walk($n - 1); }
function gen(): Generator { yield walk(300); yield walk(150); }
$total = 0;
foreach (gen() as $v) { $total += $v; }
$f = new Fiber(function (): void { Fiber::suspend(walk(300)); });
echo $total, ",", $f->start();
"#,
    );
    assert_eq!(out, "450,300");
}

/// Verifies that the OS-thread floor is restored when a fiber suspends back to the main
/// stack: deep main-stack recursion after a fiber round trip must still succeed, which it
/// cannot if `_stack_limit` were left holding the coroutine floor.
#[test]
fn test_main_stack_floor_is_restored_after_a_fiber_round_trip() {
    let out = compile_and_run(
        r#"<?php
function walk(int $n): int { if ($n <= 0) { return 0; } return 1 + walk($n - 1); }
$f = new Fiber(function (): void { Fiber::suspend(1); });
$f->start();
$f->resume();
echo walk(20000);
"#,
    );
    assert_eq!(out, "20000");
}
