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
