//! Purpose:
//! Verifies raw callable argument owners staged by descriptor invokers.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Descriptor cleanup must retire captured values on both normal and exceptional exits.
//! - Shape-validation fixtures keep the target Mixed so static signature checks cannot replace dispatch.

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
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
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
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "called|consumer|called|drop|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Boxed names, method pairs, invokable objects and descriptors share the typed callback boundary.
#[test]
fn test_descriptor_callable_arguments_normalize_php_callback_shapes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function descriptorShapeFunction(): void { echo "F"; }
class DescriptorShapeObject {
    public static function staticMethod(): void { echo "S"; }
    public function method(): void { echo "M"; }
    public function __invoke(): void { echo "I"; }
    public function __destruct() { echo "drop|"; }
}
function consumeDescriptorShape(callable $callback): void { $callback(); }
function dispatchDescriptorShape(mixed $target, mixed $callback, bool $named): void {
    if ($named) { $target(callback: $callback); } else { $target($callback); }
}
$object = new DescriptorShapeObject();
$method = $object->method(...);
$closure = function(): void { echo "C"; };
for ($i = 0; $i < 2; $i++) {
    dispatchDescriptorShape(consumeDescriptorShape(...), "DESCRIPTORSHAPEFUNCTION", $i > 0);
    dispatchDescriptorShape(consumeDescriptorShape(...), "DescriptorShapeObject::staticMethod", $i > 0);
    dispatchDescriptorShape(consumeDescriptorShape(...), ["DescriptorShapeObject", "staticMethod"], $i > 0);
    dispatchDescriptorShape(consumeDescriptorShape(...), [$object, "method"], $i > 0);
    dispatchDescriptorShape(consumeDescriptorShape(...), $object, $i > 0);
    dispatchDescriptorShape(consumeDescriptorShape(...), $method, $i > 0);
    dispatchDescriptorShape(consumeDescriptorShape(...), $closure, $i > 0);
}
unset($method, $closure, $object);
echo "done";
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "FSSMIMCFSSMIMCdrop|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Invalid boxed callbacks throw before entering the callee and leave borrowed values alive.
#[test]
fn test_descriptor_callable_argument_type_errors_retire_prepared_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class InvalidDescriptorOwner {
    public function __destruct() { echo "drop|"; }
}
function consumeInvalidDescriptor(InvalidDescriptorOwner $owner, callable $callback): void {
    echo "unexpected|";
}
function rejectDescriptorShape(mixed $target, mixed $callback, bool $named): void {
    $owner = new InvalidDescriptorOwner();
    try {
        if ($named) { $target(owner: $owner, callback: $callback); }
        else { $target($owner, $callback); }
    } catch (TypeError $error) { echo "invalid|"; }
    unset($error, $owner);
}
rejectDescriptorShape(consumeInvalidDescriptor(...), 42, false);
rejectDescriptorShape(consumeInvalidDescriptor(...), "missingDescriptorCallback", true);
rejectDescriptorShape(consumeInvalidDescriptor(...), null, false);
rejectDescriptorShape(consumeInvalidDescriptor(...), ["InvalidDescriptorOwner", "missing", "extra"], true);
echo "done";
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "invalid|drop|invalid|drop|invalid|drop|invalid|drop|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
