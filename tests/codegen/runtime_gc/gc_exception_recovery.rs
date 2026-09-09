//! Purpose:
//! Verifies destructor exceptions leave native and eval cycle collection recoverable.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - A second explicit collection must run without repeating the throwing destructor.
//! - Opaque eval covers both directions across the Rust/native exception boundary.

use crate::support::*;

/// Repeated Mixed-receiver getters retire every fresh string and array payload without leaking.
#[test]
fn test_core_mixed_throwable_getter_results_transfer_string_and_array_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function inspectMixedThrowableOwners(mixed $error): int {
    $message = $error->getMessage();
    $file = $error->getFile();
    $trace = $error->getTrace();
    $rendered = $error->__toString();
    $empty = $error->getTraceAsString();
    return strlen($message) + strlen($rendered) + count($trace)
        + strlen($empty) + (strlen($file) > 0 ? 1 : 0);
}
$total = 0;
for ($i = 0; $i < 12; $i++) {
    $error = new Exception("payload");
    $total += inspectMixedThrowableOwners($error);
    unset($error);
}
echo $total;
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "180", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Eval inspects native compact exceptions through explicit getters and owns the returned previous.
#[test]
fn test_core_eval_native_throwable_getters_preserve_previous_after_outer_unset() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function throwNativeGetterChain(): void {
    throw new RuntimeException("outer", 23, new Exception("previous", 5));
}
$source = 'try { throwNativeGetterChain(); }
catch (RuntimeException $error) {
    echo "caught|", $error->getMessage(), ":", $error->getCode(), "|";
    $previous = $error->getPrevious();
    unset($error);
    echo $previous->getMessage(), ":", $previous->getCode(), ":";
    echo $previous->getPrevious() === null ? "end" : "unexpected";
    unset($previous);
}' . ' // ' . $argc;
eval($source);
unset($source);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "caught|outer:23|previous:5:end", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A native caller retains eval's boxed previous link after releasing the outer exception.
#[test]
fn test_core_eval_throwable_previous_survives_outer_release() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function checkEvalPreviousOwner(string $source): void {
    try { eval($source); }
    catch (Exception $outer) {
        $previous = $outer->getPrevious();
        unset($outer);
        if ($previous !== null) {
            echo $previous->getMessage(), ":", $previous->getCode(), "|";
        } else {
            echo "lost-previous|";
        }
        unset($previous);
    }
}
$source = 'throw new Exception("outer", 7, new RuntimeException("inner", 13)); // ' . $argc;
checkEvalPreviousOwner($source);
checkEvalPreviousOwner($source);
unset($source);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "inner:13|inner:13|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Native exceptions caught and rethrown by opaque eval balance both ownership transfers.
#[test]
fn test_core_throwable_native_eval_round_trip_releases_both_box_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function throwNativePreviousOwner(): void { throw new RuntimeException("inner", 13); }
function checkNativeEvalPreviousOwner(string $source): void {
    try { eval($source); }
    catch (Exception $outer) {
        $previous = $outer->getPrevious();
        unset($outer);
        if ($previous !== null) { echo $previous->getMessage(), ":", $previous->getCode(), "|"; }
        else { echo "missing|"; }
        unset($previous);
    }
}
$source = 'try { throwNativePreviousOwner(); } catch (RuntimeException $inner) { throw new Exception("outer", 7, $inner); } // ' . $argc;
checkNativeEvalPreviousOwner($source);
checkNativeEvalPreviousOwner($source);
unset($source);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "inner:13|inner:13|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

const DECLARATIONS: &str = r#"
class GcThrowCycle {
    public mixed $self = null;
    public static int $calls = 0;
    public function __destruct() {
        self::$calls++;
        throw new Exception("gc-stop");
    }
}
class GcRecoveryCycle { public mixed $self = null; }
"#;

const COLLECTION: &str = r#"
gc_disable();
$cycle = new GcThrowCycle();
$cycle->self = $cycle;
unset($cycle);
try {
    gc_collect_cycles();
    echo "missed-catch|";
} catch (Exception $error) {
    echo "caught:", $error->getMessage(), "|";
}
echo gc_status()["running"] ? "busy|" : "idle|";
$next = new GcRecoveryCycle();
$next->self = $next;
unset($next);
echo gc_collect_cycles() > 0 ? "collected|" : "missed-collection|";
echo GcThrowCycle::$calls, "|";
echo gc_collect_cycles() === 0 ? "empty" : "remaining";
"#;

/// Checks the shared catch, collector-state, second-pass, and destructor-once assertions.
fn assert_gc_exception_recovery(source: &str) {
    assert_eq!(compile_and_run(source), "caught:gc-stop|idle|collected|1|empty");
}

/// A native destructor throwing during collection does not latch the collector-active flag.
#[test]
fn test_core_gc_native_destructor_exception_allows_later_collection() {
    assert_gc_exception_recovery(&format!("<?php {DECLARATIONS} {COLLECTION}"));
}

/// An eval-defined destructor's Throwable reaches its eval catch and is not silently discarded.
#[test]
fn test_core_gc_eval_destructor_exception_reaches_catch() {
    assert_gc_exception_recovery(&format!(
        "<?php $source = '{DECLARATIONS} {COLLECTION}' . ' // ' . $argc; eval($source);",
    ));
}

/// Eval catches a native destructor's Throwable without a longjmp bypassing Rust frames.
#[test]
fn test_core_gc_eval_collects_native_throwing_destructor() {
    assert_gc_exception_recovery(&format!(
        "<?php {DECLARATIONS} $source = '{COLLECTION}' . ' // ' . $argc; eval($source);",
    ));
}

/// Throwing destructors complete the candidate batch and preserve both errors in the chain.
#[test]
fn test_core_gc_multiple_destructor_exceptions_preserve_chain() {
    let source = r#"<?php
class GcChainedThrowCycle {
    public mixed $self = null;
    public string $name;
    public static int $calls = 0;
    public function __construct(string $name) { $this->name = $name; }
    public function __destruct() {
        self::$calls++;
        throw new Exception($this->name);
    }
}

gc_disable();
$first = new GcChainedThrowCycle("first");
$second = new GcChainedThrowCycle("second");
$first->self = $first;
$second->self = $second;
unset($first, $second);
try {
    gc_collect_cycles();
    echo "missed-catch";
} catch (Exception $error) {
    $previous = $error->getPrevious();
    echo GcChainedThrowCycle::$calls, ":";
    if ($previous !== null) {
        echo $previous->getPrevious() === null
            && $error->getMessage() !== $previous->getMessage() ? "chain|" : "bad-chain|";
    } else {
        echo "missing-chain|";
    }
}
echo gc_status()["running"] ? "busy|" : "idle|";
gc_collect_cycles();
echo GcChainedThrowCycle::$calls;
"#;
    assert_eq!(compile_and_run(source), "2:chain|idle|2");
}

/// Ordinary Throwable subclasses keep their boxed previous slots and pre-existing chains during GC.
#[test]
fn test_core_gc_destructor_exception_chains_keep_subclass_previous_links() {
    let source = r#"<?php
class GcChainedException extends Exception { public int $marker = 42; }
class GcExistingChainCycle {
    public mixed $link = null;
    public string $name;
    public function __construct(string $name) { $this->name = $name; }
    public function __destruct() {
        throw new GcChainedException($this->name, 0, new Exception("original:" . $this->name));
    }
}
gc_disable();
$a = new GcExistingChainCycle("a");
$b = new GcExistingChainCycle("b");
$a->link = $a;
$b->link = $b;
unset($a, $b);
try {
    gc_collect_cycles();
    echo "missed-catch";
} catch (Exception $error) {
    $first = $error->getMessage();
    $original = $error->getPrevious();
    $older = $original->getPrevious();
    echo get_class($error), ":";
    echo $original->getMessage() === ("original:" . $first) ? "kept|" : "bad|";
    echo get_class($older), ":";
    echo $older->getPrevious()->getMessage() === ("original:" . $older->getMessage()) ? "kept|" : "bad|";
    echo $older->getPrevious()->getPrevious() === null ? "end|" : "bad|";
    echo gc_status()["running"] ? "running" : "idle";
}
"#;
    assert_eq!(compile_and_run(source), "GcChainedException:kept|GcChainedException:kept|end|idle");
}
