//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of fibers errors, including fiber error on suspend outside fiber, fiber error on start twice, and fiber error on resume terminated.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies that `Fiber::suspend()` outside a fiber context throws `FiberError`
/// with the message "Cannot suspend outside of a fiber".
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

/// Verifies that calling `start()` on a fiber that has already been started
/// throws `FiberError` with the message "Cannot start a fiber that has already been started".
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

/// Verifies that calling `resume()` on a terminated (not suspended) fiber throws
/// `FiberError` with the message "Cannot resume a fiber that is not suspended".
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

/// Verifies that calling `getReturn()` on a fiber that is still running (has not
/// returned) throws `FiberError` with the message "Cannot get fiber return value: The fiber has not returned".
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

/// Verifies that calling `getReturn()` on a fiber that has never been started throws
/// `Error` (not `Exception`), caught by the `Error` handler in a try/catch.
#[test]
fn test_fiber_error_on_get_return_is_caught_by_error() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {});
try {
    $f->getReturn();
} catch (Error $e) {
    echo "error";
} catch (Exception $e) {
    echo "exception";
}
"#,
    );
    assert_eq!(out, "error");
}

/// Verifies that calling `throw()` on a fiber that is not suspended throws `FiberError`
/// with the message "Cannot resume a fiber that is not suspended".
///
/// The thrown value is an `Exception` rather than a `FiberError`: reference PHP reserves
/// `FiberError` for internal use, so the original fixture's `new FiberError("x")` is refused
/// there. Which throwable is handed to `throw()` is incidental to this test — the fiber is
/// not suspended, so it never reaches the callback.
#[test]
fn test_fiber_error_on_throw_not_suspended() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {});
try { $f->throw(new Exception("x")); echo "no-throw"; }
catch (FiberError $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "Cannot resume a fiber that is not suspended");
}

/// Verifies that an uncaught exception thrown inside a fiber escapes to the caller
/// via `start()`, producing output "caught:from fiber".
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

/// Verifies that an exception thrown via `throw()` into a suspended fiber escapes
/// to the caller when the fiber does not catch it, producing output "caught:via throw".
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

/// Verifies that an internal `catch` block inside a fiber suppresses an exception and
/// execution continues normally after `start()`, producing "fiber-caught;after-start".
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

/// Verifies the reserved-class refusal did not take the engine's own `FiberError` with it.
///
/// `new FiberError(...)` is refused at compile time, which is what PHP does — see
/// `test_error_fiber_error_is_reserved_for_internal_use`. The class must still exist for the
/// runtime to RAISE and for user code to CATCH by name; a guard that removed it outright
/// would satisfy the refusal test and break every fiber program.
#[test]
fn test_fiber_error_is_still_raised_and_catchable() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function (): void { Fiber::suspend(1); });
$f->start();
try { $f->start(); echo "not reached"; }
catch (FiberError $e) { echo "caught:", get_class($e); }
"#,
    );
    assert_eq!(out, "caught:FiberError");
}
