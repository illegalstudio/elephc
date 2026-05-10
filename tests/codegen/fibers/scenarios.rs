//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of fibers scenarios, including fiber php constructs inside body, fiber canonical php doc example, and fiber closure capture string survives suspend.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_fiber_php_constructs_inside_body() {
    // try/finally, foreach, match, and `new` all work inside a fiber's
    // closure body, including across a suspend boundary in the try block.
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

#[test]
fn test_fiber_canonical_php_doc_example() {
    // The canonical example from the PHP documentation, adapted for elephc's
    // mixed-payload-only suspend/resume surface (strings round-trip cleanly).
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

#[test]
fn test_fiber_error_subclasses_exception() {
    let out = compile_and_run(
        r#"<?php
try {
    throw new FiberError("nope");
} catch (Exception $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

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
