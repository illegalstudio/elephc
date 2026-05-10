//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of fibers errors, including fiber error on suspend outside fiber, fiber error on start twice, and fiber error on resume terminated.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_fiber_error_on_suspend_outside_fiber() {
    let out = compile_and_run(
        r#"<?php
try { Fiber::suspend(0); echo "no-throw"; }
catch (FiberError $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "Cannot suspend outside of a fiber");
}

#[test]
fn test_fiber_error_on_start_twice() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {});
$f->start();
try { $f->start(); echo "no-throw"; }
catch (FiberError $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "Cannot start a fiber that has already been started");
}

#[test]
fn test_fiber_error_on_resume_terminated() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {});
$f->start();
try { $f->resume(0); echo "no-throw"; }
catch (FiberError $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "Cannot resume a fiber that is not suspended");
}

#[test]
fn test_fiber_error_on_get_return_before_terminated() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void { Fiber::suspend(0); });
$f->start();
try { $f->getReturn(); echo "no-throw"; }
catch (FiberError $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "Cannot get fiber return value: The fiber has not returned");
}

#[test]
fn test_fiber_error_on_throw_not_suspended() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {});
try { $f->throw(new FiberError("x")); echo "no-throw"; }
catch (FiberError $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "Cannot resume a fiber that is not suspended");
}

#[test]
fn test_fiber_uncaught_exception_escapes_to_caller() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    throw new Exception("from fiber");
});
try { $f->start(); echo "no-throw"; }
catch (Exception $e) { echo "caught:" . $e->getMessage(); }
"#,
    );
    assert_eq!(out, "caught:from fiber");
}

#[test]
fn test_fiber_throw_escapes_when_fiber_does_not_catch() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    Fiber::suspend(0);
});
$f->start();
try { $f->throw(new Exception("via throw")); echo "no-throw"; }
catch (Exception $e) { echo "caught:" . $e->getMessage(); }
"#,
    );
    assert_eq!(out, "caught:via throw");
}

#[test]
fn test_fiber_internal_catch_does_not_escape() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    try { throw new Exception("internal"); }
    catch (Exception $e) { echo "fiber-caught;"; }
});
$f->start();
echo "after-start";
"#,
    );
    assert_eq!(out, "fiber-caught;after-start");
}

