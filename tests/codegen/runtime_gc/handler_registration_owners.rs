//! Purpose:
//! Verifies temporary callback owners created by native error and exception handler registration.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Registrations retain callbacks independently; preparation temporaries must not retain them forever.

use crate::support::*;

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
