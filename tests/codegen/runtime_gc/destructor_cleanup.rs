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
