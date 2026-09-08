//! Purpose:
//! Checks exception-safe final-owner cleanup across native heap containers and callable captures.
//!
//! Called from:
//! - The runtime GC codegen suite on each executable target.
//!
//! Key details:
//! - Destructors throw after allocating owned fields, so output alone cannot hide interrupted cleanup.
//! - The GC protection flag is checked separately from a later explicit collection.

use crate::support::*;

/// A throw to the caller releases every owned local in an ordinary native executable frame.
#[test]
fn test_core_native_unwind_releases_owned_frame_locals() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NativeUnwindOwner {
    public static int $destroyed = 0;
    public int $value = 7;
    public function __destruct() { self::$destroyed++; }
}
function abortOwnedNativeFrame(Exception $exception): void {
    $first = new NativeUnwindOwner();
    $second = new NativeUnwindOwner();
    echo $first->value + $second->value, ":";
    throw $exception;
}
$exception = new Exception("original");
for ($i = 0; $i < 3; $i++) {
    try { abortOwnedNativeFrame($exception); }
    catch (Exception $caught) { echo $caught->getMessage(), "|"; unset($caught); }
}
unset($exception);
echo NativeUnwindOwner::$destroyed;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "14:original|14:original|14:original|6", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Destructor throws during unwinding preserve the original exception and finish sibling owners.
#[test]
fn test_core_native_unwind_finishes_throwing_sibling_locals() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NativeUnwindFirst {
    public function __destruct() { echo "first|"; throw new RuntimeException("first"); }
}
class NativeUnwindSecond {
    public function __destruct() { echo "second|"; throw new RuntimeException("second"); }
}
function abortThrowingNativeFrame(): void {
    $first = new NativeUnwindFirst();
    $second = new NativeUnwindSecond();
    echo get_class($first), ":", get_class($second), "|";
    throw new Exception("original");
}
try { abortThrowingNativeFrame(); }
catch (Throwable $caught) {
    echo $caught->getMessage(), ":", $caught->getPrevious()->getMessage(), ":";
    echo $caught->getPrevious()->getPrevious()->getMessage();
    unset($caught);
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "NativeUnwindFirst:NativeUnwindSecond|first|second|second:first:original", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A throwing last-owner destructor must not leak the promoted local reference cell.
#[test]
fn test_core_throwing_reference_payload_retires_its_local_cell() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReferenceCellThrowingValue {
    public int $value = 7;
    public static RuntimeException $error;
    public function __destruct() { throw self::$error; }
}
function retireThrowingReferenceCell(): void {
    $value = new ReferenceCellThrowingValue();
    $alias =& $value;
    echo $alias->value, ":";
}
ReferenceCellThrowingValue::$error = new RuntimeException("reference");
for ($i = 0; $i < 3; $i++) {
    try { retireThrowingReferenceCell(); }
    catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "7:reference|7:reference|7:reference|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Every container finishes sibling cleanup and preserves both exceptions before freeing its storage.
#[test]
fn test_core_throwing_destructors_release_objects_arrays_hashes_and_callable_captures() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ThrowingCleanupChild {
    public string $message;
    public string $buffer;
    public function __construct(string $message) {
        $this->message = $message;
        $this->buffer = str_repeat("x", 48);
    }
    public function __destruct() { throw new Exception($this->message); }
}
class ThrowingCleanupParent {
    public ThrowingCleanupChild $first;
    public ThrowingCleanupChild $second;
    public function __construct() {
        $this->first = new ThrowingCleanupChild("first");
        $this->second = new ThrowingCleanupChild("second");
    }
}
function releaseCleanupObject(): void {
    $value = new ThrowingCleanupParent();
    unset($value);
}
function releaseCleanupArray(): void {
    $value = [new ThrowingCleanupChild("first"), new ThrowingCleanupChild("second")];
    unset($value);
}
function releaseCleanupHash(): void {
    $value = ["first" => new ThrowingCleanupChild("first"), "second" => new ThrowingCleanupChild("second")];
    unset($value);
}
function releaseCleanupCallable(): void {
    $first = new ThrowingCleanupChild("first");
    $second = new ThrowingCleanupChild("second");
    $value = static function () use ($first, $second): void {};
    unset($first, $second);
    unset($value);
}
function describeCleanupChain(Throwable $error): void {
    $previous = $error->getPrevious();
    echo $previous !== null && $previous->getPrevious() === null ? "chain|" : "bad|";
}
try { releaseCleanupObject(); } catch (Throwable $error) { describeCleanupChain($error); unset($error); }
try { releaseCleanupArray(); } catch (Throwable $error) { describeCleanupChain($error); unset($error); }
try { releaseCleanupHash(); } catch (Throwable $error) { describeCleanupChain($error); unset($error); }
try { releaseCleanupCallable(); } catch (Throwable $error) { describeCleanupChain($error); unset($error); }
$status = gc_status();
echo $status["protected"] ? "suppressed|" : "unprotected|";
unset($status);
class CleanupLaterCycle { public mixed $self = null; }
gc_disable();
$cycle = new CleanupLaterCycle();
$cycle->self = $cycle;
unset($cycle);
echo gc_collect_cycles() > 0 ? "collected" : "suppressed";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "chain|chain|chain|chain|unprotected|collected", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
