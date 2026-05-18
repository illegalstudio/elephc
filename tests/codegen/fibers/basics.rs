//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of fibers basics, including fiber construction does not crash, fiber state predicates initial, and fiber get current returns null outside fiber.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_fiber_construction_does_not_crash() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {});
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_fiber_state_predicates_initial() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {});
if ($f->isStarted()) { echo "S"; } else { echo "s"; }
if ($f->isRunning()) { echo "R"; } else { echo "r"; }
if ($f->isSuspended()) { echo "P"; } else { echo "p"; }
if ($f->isTerminated()) { echo "T"; } else { echo "t"; }
"#,
    );
    assert_eq!(out, "srpt");
}

#[test]
fn test_fiber_get_current_returns_null_outside_fiber() {
    // From outside any fiber, getCurrent returns the boxed null Mixed cell.
    // is_null() narrows back to a clean boolean we can assert on.
    let out = compile_and_run(
        r#"<?php
echo is_null(Fiber::getCurrent()) ? "null" : "not-null";
"#,
    );
    assert_eq!(out, "null");
}

#[test]
fn test_fiber_get_current_inside_is_boxed_fiber_object() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    $cur = Fiber::getCurrent();
    echo ($cur instanceof Fiber) ? "fiber" : "not-fiber";
    echo "/";
    echo $cur->isRunning() ? "running" : "not-running";
});
$f->start();
"#,
    );
    assert_eq!(out, "fiber/running");
}

#[test]
fn test_fiber_runs_to_completion() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void { echo "inside"; });
$f->start();
echo "|after";
if ($f->isTerminated()) { echo "|term"; }
"#,
    );
    assert_eq!(out, "inside|after|term");
}

#[test]
fn test_fiber_suspend_returns_value_to_caller() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    Fiber::suspend(42);
});
$r = $f->start();
echo $r;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_fiber_suspend_without_value_yields_null() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    Fiber::suspend();
});
$r = $f->start();
echo is_null($r) ? "null" : "not-null";
"#,
    );
    assert_eq!(out, "null");
}

#[test]
fn test_fiber_resume_delivers_value_to_suspend() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    $v = Fiber::suspend(0);
    echo "got=" . $v;
});
$f->start();
$f->resume(99);
"#,
    );
    assert_eq!(out, "got=99");
}

#[test]
fn test_fiber_resume_delivers_nested_array_to_suspend() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    $arr = ["b" => [10, 20, 30]];
    $x = Fiber::suspend($arr);
    echo $x["b"][1];
});
$a = $f->start();
$f->resume(["b" => [99, 77]]);
"#,
    );
    assert_eq!(out, "77");
}

#[test]
fn test_fiber_full_suspend_resume_cycle() {
    // Mixed-tagged values flow through `transfer_value`. We echo each
    // received Mixed payload directly without arithmetic so the test does
    // not depend on Mixed-cell arithmetic auto-unboxing.
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    $a = Fiber::suspend("yield-1");
    echo "[got " . $a . "]";
    $b = Fiber::suspend("yield-2");
    echo "[got " . $b . "]";
    Fiber::suspend("yield-3");
});
echo $f->start();
echo "|";
echo $f->resume("resume-A");
echo "|";
echo $f->resume("resume-B");
"#,
    );
    assert_eq!(out, "yield-1|[got resume-A]yield-2|[got resume-B]yield-3");
}

#[test]
fn test_fiber_terminal_return_available_only_from_get_return() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): mixed {
    return "ret";
});
$v = $f->start();
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "null/ret");
}

#[test]
fn test_fiber_resume_returns_null_when_fiber_terminates() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): mixed {
    Fiber::suspend("yield");
    return "ret";
});
echo $f->start();
echo "/";
$v = $f->resume("go");
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "yield/null/ret");
}

#[test]
fn test_fiber_int_return_is_boxed_for_get_return() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): int {
    return 42;
});
$v = $f->start();
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
    );
    assert_eq!(out, "null/42");
}

#[test]
fn test_fiber_stack_is_released_when_object_is_freed() {
    // Each fiber owns a 256 KB stack from the heap (default 8 MB). Reassigning
    // $f over 50 iterations drops the previous Fiber's refcount to 0; the
    // object_free_deep hook must release its stack or we run out of heap
    // before iteration 32 and abort with "Fatal error: heap memory exhausted".
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 50; $i++) {
    $f = new Fiber(function(): void {});
    $f->start();
}
echo "iters=" . $i;
"#,
    );
    assert_eq!(out, "iters=50");
}
