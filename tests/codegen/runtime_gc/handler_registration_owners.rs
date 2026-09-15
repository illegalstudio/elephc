//! Purpose:
//! Verifies temporary callback owners created by native error and exception handler registration.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Registrations retain callbacks independently; preparation temporaries must not retain them forever.
//! - Handler getters hand out one extra owner per call and never touch the descriptor owner.

use crate::support::*;

/// Throwing captured destructors do not skip previous-handler restoration or leave detached owners.
#[test]
fn test_core_handler_restore_finishes_cleanup_before_propagating_destructor_throw() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ThrowingHandlerOwner {
    public function __destruct() { echo "drop|"; throw new Exception("release"); }
}
set_error_handler(function(int $level, string $message): bool { echo "previous|"; return true; });
$owner = new ThrowingHandlerOwner();
set_error_handler(function(int $level, string $message) use ($owner): bool { return true; });
unset($owner);
try { restore_error_handler(); } catch (Exception $error) { echo $error->getMessage(), "|"; }
trigger_error("after", E_USER_WARNING);
restore_error_handler();
set_exception_handler(function(Throwable $error): void { echo "exception|"; });
$owner = new ThrowingHandlerOwner();
set_exception_handler(function(Throwable $error) use ($owner): void {});
unset($owner);
try { restore_exception_handler(); } catch (Exception $error) { echo $error->getMessage(), "|"; }
$previous = set_exception_handler(null);
$previous(new Exception("after"));
restore_exception_handler();
restore_exception_handler();
unset($previous, $error);
echo "done";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "drop|release|previous|drop|release|exception|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Restoring a temporary error handler destroys its captured owner after the registration releases it.
#[test]
fn test_core_error_handler_registration_releases_internal_temporaries() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class HandlerCaptureOwner {
    public function __destruct() { echo "drop|"; }
}
for ($i = 0; $i < 3; $i++) {
    $owner = new HandlerCaptureOwner();
    set_error_handler(function(int $level, string $message) use ($owner): bool { echo "handled|"; return true; });
    unset($owner);
    trigger_error("message", E_USER_WARNING);
    restore_error_handler();
}
echo "done";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "handled|drop|handled|drop|handled|drop|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Exception handler preparation does not consume boxed callbacks or leak its normalized descriptor.
#[test]
fn test_core_exception_handler_registration_preserves_boxed_callback_owner() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ExceptionHandlerOwner {
    public function __destruct() { echo "drop|"; }
}
function installExceptionCallback(mixed $callback): void {
    set_exception_handler($callback);
    restore_exception_handler();
}
$owner = new ExceptionHandlerOwner();
$callback = function(Throwable $error) use ($owner): void { echo "called|"; };
unset($owner);
installExceptionCallback($callback);
$callback(new Exception("after"));
unset($callback);
set_exception_handler(function(Throwable $error): void {});
restore_exception_handler();
echo "done";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "called|drop|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Getter results are independent owners: they keep a captured object alive past restoration
/// and release it exactly once, while the registration's own descriptor stays untouched.
#[test]
fn test_core_handler_getters_return_independent_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class GetterCaptureOwner {
    public function __destruct() { echo "drop|"; }
}
echo is_null(get_error_handler()) && is_null(get_exception_handler()) ? "null|" : "bad|";
$owner = new GetterCaptureOwner();
set_error_handler(function(int $level, string $message) use ($owner): bool { echo "handled|"; return true; });
unset($owner);
$first = get_error_handler();
$second = get_error_handler();
echo $first === $second ? "same|" : "bad|";
unset($second);
restore_error_handler();
echo is_null(get_error_handler()) ? "cleared|" : "bad|";
$first(E_USER_WARNING, "manual");
unset($first);
echo "released|";
$owner = new GetterCaptureOwner();
set_exception_handler(function(Throwable $error) use ($owner): void { echo "exception|"; });
unset($owner);
for ($i = 0; $i < 3; $i++) {
    $copy = get_exception_handler();
    unset($copy);
}
$kept = get_exception_handler();
set_exception_handler(null);
echo is_null(get_exception_handler()) ? "none|" : "bad|";
restore_exception_handler();
restore_exception_handler();
$kept(new Exception("manual"));
unset($kept);
echo "done";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "null|same|cleared|handled|drop|released|none|exception|drop|done",
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
