//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of fibers scenarios, including fiber php constructs inside body, fiber canonical php doc example, and fiber closure capture string survives suspend.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies try/finally, foreach, match, and `new` work inside a fiber body across a suspend boundary.
#[test]
fn test_fiber_php_constructs_inside_body() {
    let out = compile_and_run(
        r#"<?php
class Item { public string $name = ""; }
$f = new Fiber(function(): void {
    try {
        echo "T;";
        Fiber::suspend(0);
        echo "A;";
    } finally {
        echo "F;";
    }
    $items = [10, 20, 30];
    foreach ($items as $v) { echo "v" . $v . ";"; }
    $x = 2;
    $r = match ($x) { 1 => "one", 2 => "two", default => "other" };
    echo "m" . $r . ";";
    $i = new Item();
    $i->name = "widget";
    echo "o" . $i->name;
});
$f->start();
$f->resume(0);
"#,
    );
    assert_eq!(out, "T;A;F;v10;v20;v30;mtwo;owidget");
}

/// Verifies the canonical PHP documentation example for fibers: suspend with a string value and resume with a different value.
#[test]
fn test_fiber_canonical_php_doc_example() {
    let out = compile_and_run(
        r#"<?php
$fiber = new Fiber(function(): void {
    $value = Fiber::suspend("fiber");
    echo "Value used to resume fiber: " . $value;
});
$value = $fiber->start();
echo "Value from fiber suspending: " . $value . "|";
$fiber->resume("test");
"#,
    );
    assert_eq!(out, "Value from fiber suspending: fiber|Value used to resume fiber: test");
}

/// Verifies closure-captured string variables survive a suspend/resume cycle.
#[test]
fn test_fiber_closure_capture_string_survives_suspend() {
    let out = compile_and_run(
        r#"<?php
$ctx = "stable";
$f = new Fiber(function() use ($ctx): void {
    Fiber::suspend(0);
    echo "after=" . $ctx;
});
$f->start();
$f->resume(0);
"#,
    );
    assert_eq!(out, "after=stable");
}

/// Verifies closure-captured integer variables survive a suspend/resume cycle.
#[test]
fn test_fiber_closure_capture_survives_suspend_resume() {
    let out = compile_and_run(
        r#"<?php
$base = 100;
$f = new Fiber(function() use ($base): void {
    Fiber::suspend(0);
    echo "after-resume base=" . $base;
});
$f->start();
$f->resume(0);
"#,
    );
    assert_eq!(out, "after-resume base=100");
}

/// Verifies string payloads round-trip correctly through suspend and start (no data corruption).
#[test]
fn test_fiber_string_payload_round_trip() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    Fiber::suspend("hello");
});
echo $f->start();
"#,
    );
    assert_eq!(out, "hello");
}

/// Verifies Fiber state transitions: Start → Suspended → Terminated, checking isStarted/isSuspended/isTerminated at each stage.
#[test]
fn test_fiber_state_transitions() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void { Fiber::suspend(0); });
echo $f->isStarted() ? "S" : "s";
$f->start();
echo $f->isStarted() ? "S" : "s";
echo $f->isSuspended() ? "P" : "p";
echo $f->isTerminated() ? "T" : "t";
$f->resume(0);
echo $f->isTerminated() ? "T" : "t";
"#,
    );
    assert_eq!(out, "sSPtT");
}

/// Verifies FiberError is a subclass of Error (catchable by Error, not Exception).
#[test]
fn test_fiber_error_subclasses_error() {
    let out = compile_and_run(
        r#"<?php
try {
    throw new FiberError("nope");
} catch (Exception $e) {
    echo "exception";
} catch (Error $e) {
    echo "error";
}
"#,
    );
    assert_eq!(out, "error");
}

/// Verifies FiberError is caught by its specific type before Exception.
#[test]
fn test_fiber_error_caught_by_specific_type() {
    let out = compile_and_run(
        r#"<?php
try {
    throw new FiberError("x");
} catch (FiberError $e) {
    echo "fiber-err";
} catch (Exception $e) {
    echo "exc";
}
"#,
    );
    assert_eq!(out, "fiber-err");
}

/// Verifies Fiber:: throw delivers a FiberError to a fiber's internal try/catch, and execution resumes after the catch block.
#[test]
fn test_fiber_throw_caught_by_internal_try_catch() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    echo "1";
    try {
        Fiber::suspend(0);
        echo "X-not-reached";
    } catch (FiberError $e) {
        echo "2";
    }
    echo "3";
});
echo "A";
$f->start();
echo "B";
$f->throw(new FiberError("delivered"));
echo "C";
"#,
    );
    assert_eq!(out, "A1B23C");
}
