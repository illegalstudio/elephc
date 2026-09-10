//! Purpose:
//! Verifies raw callable argument owners staged by descriptor invokers.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Descriptor cleanup must retire captured values on both normal and exceptional exits.

use crate::support::*;

/// Positional and named callback arguments release their descriptor leases after native calls.
#[test]
fn test_core_descriptor_callable_arguments_release_captured_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class DescriptorCallbackOwner {
    public function __destruct() { echo "drop|"; }
}
function consumeDescriptorCallback(callable $callback): void { $callback(); }
function dispatchDescriptorCallback(callable $target, callable $callback, bool $named): void {
    if ($named) { $target(callback: $callback); } else { $target($callback); }
}
for ($i = 0; $i < 3; $i++) {
    $owner = new DescriptorCallbackOwner();
    $callback = function() use ($owner): void { echo "called|"; };
    unset($owner);
    dispatchDescriptorCallback(consumeDescriptorCallback(...), $callback, $i > 0);
    unset($callback);
}
echo "done";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "called|drop|called|drop|called|drop|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A throwing consumer releases its invocation lease without consuming the caller's callable.
#[test]
fn test_core_descriptor_callable_arguments_release_on_native_throw() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ThrowingDescriptorCallbackOwner {
    public function __destruct() { echo "drop|"; }
}
function throwWithDescriptorCallback(callable $callback): void {
    $callback();
    throw new Exception("consumer");
}
function dispatchThrowingDescriptor(callable $target, callable $callback): void { $target(callback: $callback); }
$owner = new ThrowingDescriptorCallbackOwner();
$callback = function() use ($owner): void { echo "called|"; };
unset($owner);
try { dispatchThrowingDescriptor(throwWithDescriptorCallback(...), $callback); }
catch (Exception $error) { echo $error->getMessage(), "|"; }
$callback();
unset($callback, $error);
echo "done";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "called|consumer|called|drop|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
