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
// Verifies that constructing a Fiber with an empty callable does not crash the compiler/runtime.
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
// Verifies that a newly constructed fiber reports the correct initial state predicates:
// isStarted=false, isRunning=false, isSuspended=false, isTerminated=false.
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
// Verifies that Fiber::getCurrent() returns null (a boxed null Mixed cell) when called
// outside of any fiber. Uses is_null() for a clean boolean assertion.
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
// Verifies that Fiber::getCurrent() inside a running fiber returns the Fiber object
// itself (boxed as a Mixed cell), and that isRunning() is true on that object.
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
// Verifies that a fiber runs to completion when started, that control returns to the
// caller after the fiber function exits, and that isTerminated() is true after finish.
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
// Verifies that Fiber::suspend(value) returns the value to the caller's start() return.
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
// Verifies that Fiber::suspend() with no argument yields null to the caller.
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
// Verifies that resume(value) delivers a value to the suspend call that receives it.
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
// Verifies that resume delivers a nested array value to the suspend call and that
// array indexing into the received value works correctly (regression for Mixed
// cell payload handling).
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
// Verifies a full suspend-resume cycle across three sequential suspend/resume pairs,
// confirming Mixed-tagged values flow correctly through transfer_value.
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
// Verifies that the fiber return value is only available via getReturn(), not via
// start() return, and that is_null() correctly identifies the null start() return.
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
// Verifies that resume() returns null when the fiber terminates (after the final
// suspend), and that getReturn() still yields the terminal return value.
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
// Verifies that an int return type produces the correct integer via getReturn(), and
// that the start() return is null.
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
// Verifies that fiber stacks are released when Fiber objects are freed. Reassigning
// $f over 50 iterations drops each Fiber's refcount to 0; object_free_deep must
// release the stack or the process aborts with "heap memory exhausted".
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

#[test]
// Verifies that a Fiber stored in a Mixed-typed property is released when the
// property is nulled and the object is reset. Uses a reduced heap to surface
// any missing cleanup.
fn test_fiber_stored_in_mixed_property_is_released_on_reset() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
class Slot {
    public $fiber = null;

    public function cycle(): void {
        $this->fiber = new Fiber(function(): void {});
        $this->fiber->start();
        $this->fiber = null;
    }
}

$s = new Slot();
for ($i = 0; $i < 300; $i++) {
    $s->cycle();
}
echo "iters=" . $i;
"#,
        65_536,
    );
    assert_eq!(out, "iters=300");
}

#[test]
// Verifies that a Fiber capturing $this in a closure is released when the property
// is nulled and the cycle completes. Uses a reduced heap to surface any missing cleanup.
fn test_fiber_capture_cycle_is_released_on_property_reset() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
class Slot {
    public $fiber = null;
    public bool $closed = false;

    public function cycle(): void {
        $self = $this;
        $this->closed = false;
        $this->fiber = new Fiber(function() use ($self): void {
            $self->closed = true;
        });
        $this->fiber->start();
        $this->fiber = null;
    }
}

$s = new Slot();
for ($i = 0; $i < 300; $i++) {
    $s->cycle();
}
echo "iters=" . $i;
"#,
        65_536,
    );
    assert_eq!(out, "iters=300");
}

#[test]
// Verifies that unset($s) after a cycle with a closure-capturing Fiber leaves zero
// live heap blocks (allocs == frees). Confirms there are no reference cycles preventing
// deallocation.
fn test_fiber_capture_cycle_reset_leaves_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
class Slot {
    public $fiber = null;
    public bool $closed = false;

    public function cycle(): void {
        $self = $this;
        $this->closed = false;
        $this->fiber = new Fiber(function() use ($self): void {
            $self->closed = true;
        });
        $this->fiber->start();
        $this->fiber = null;
    }
}

$s = new Slot();
for ($i = 0; $i < 3; $i++) {
    $s->cycle();
}
unset($s);
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

#[test]
// Verifies that discarding a Fiber (via unset) after start() leaves zero live heap
// blocks (allocs == frees). Confirms start() result cleanup.
fn test_discarded_fiber_start_result_leaves_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
for ($i = 0; $i < 3; $i++) {
    $f = new Fiber(function(): void {});
    $f->start();
    unset($f);
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

#[test]
// Verifies that a Fiber with a suspend value, when discarded (unset) after start(),
// does not double-free the suspend value. Uses heap debug to catch double-free.
fn test_discarded_suspend_value_is_not_released_again_with_fiber() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Slot {
    public $fiber = null;

    public function cycle(): void {
        $this->fiber = new Fiber(function(): void {
            Fiber::suspend("yielded");
        });
        $this->fiber->start();
        $this->fiber = null;
    }
}

$s = new Slot();
for ($i = 0; $i < 20; $i++) {
    $s->cycle();
}
unset($s);
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

#[test]
// Verifies that resume(value) with an active fiber does not double-free that value
// when the fiber is later discarded. Uses heap debug to catch double-free.
fn test_resume_value_is_not_released_again_with_fiber() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($i = 0; $i < 20; $i++) {
    $f = new Fiber(function(): void {
        $value = Fiber::suspend("ready");
        echo $value === "resume" ? "" : "bad";
    });
    $f->start();
    $f->resume("resume");
    unset($f);
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

#[test]
// Verifies that getReturn() value survives beyond Fiber object destruction (unset).
// Confirms the return value is not tied to the fiber's heap lifetime.
fn test_fiber_get_return_result_survives_fiber_release() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$f = new Fiber(function(): mixed {
    return "ret";
});
$f->start();
$value = $f->getReturn();
unset($f);
echo $value;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ret");
}
