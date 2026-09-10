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

/// Returns one bounded user-function region for ownership diagnostics on CI failures.
fn bounded_user_function_assembly(assembly: &str, function: &str) -> String {
    let label = format!("_fn_{function}:\n");
    let Some((_, tail)) = assembly.split_once(&label) else {
        return "<function assembly not found>".to_string();
    };
    let mut body = label;
    for line in tail.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(".globl ") || trimmed.starts_with(".global ") {
            break;
        }
        let line = line.split_once(" // ").map_or(line, |(instruction, _)| instruction);
        let line = line.split_once(" # ").map_or(line, |(instruction, _)| instruction);
        if !line.trim().is_empty()
            && !line.trim_start().starts_with("//")
            && !matches!(line.trim_start().as_bytes().first(), Some(b'#'))
        {
            body.push_str(line);
            body.push('\n');
        }
    }
    body.chars().take(128_000).collect()
}

/// Failed and matched native catch predicates release the temporary boxes used to query eval.
#[test]
fn test_core_eval_catch_predicate_boxes_release_mismatched_and_unbound_throwables() {
    let source = r#"<?php
function inspectEvalCatchPredicateOwners(string $source): void {
    try { eval($source); }
    catch (LogicException $wrong) { echo "wrong|"; }
    catch (RuntimeException $right) { echo $right->getMessage(), "|"; unset($right); }
    try { eval($source); }
    catch (LogicException) { echo "wrong|"; }
    catch (RuntimeException) { echo "unbound|"; }
}
$source = 'throw new RuntimeException("right"); // ' . $argc;
for ($i = 0; $i < 3; $i++) { inspectEvalCatchPredicateOwners($source); }
unset($source);
"#;
    let expected = "right|unbound|".repeat(3);
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Post-eval metadata queries release adapter boxes without consuming the caller's object owner.
#[test]
fn test_core_eval_metadata_queries_balance_native_and_borrowed_object_inputs() {
    let source = r#"<?php
class MetadataOwnerBase { public int $value = 7; public function method(): void {} }
class MetadataOwnerChild extends MetadataOwnerBase { public function __destruct() { echo "D|"; } }
function inspectEvalMetadataOwners(MetadataOwnerChild $object, string $source): void {
    eval($source);
    $target = get_parent_class($object);
    echo $object instanceof MetadataOwnerBase ? "named:" : "wrong:";
    echo $object instanceof $target ? "dynamic:" : "wrong:";
    echo get_class($object), ":", $target, ":";
    echo method_exists($object, "method"), ":", property_exists($object, "value"), ":";
    echo is_callable([$object, "method"]), ":";
    echo count(class_parents($object)), ":", count(class_implements($object)), ":", count(class_uses($object)), "|";
}
$source = 'return null; // ' . $argc;
for ($i = 0; $i < 3; $i++) {
    $object = new MetadataOwnerChild();
    inspectEvalMetadataOwners($object, $source);
    echo $object->value, "|";
    unset($object);
}
unset($source);
"#;
    let expected = "named:dynamic:MetadataOwnerChild:MetadataOwnerBase:1:1:1:1:0:0|7|D|".repeat(3);
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}\ninspectEvalMetadataOwners assembly:\n{}", out.stderr,
        bounded_user_function_assembly(&assembly, "inspectEvalMetadataOwners"));
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Focused eval callable and metadata results release temporary cells without consuming objects.
#[test]
fn test_core_eval_callable_and_nonempty_metadata_results_balance_owners() {
    let source = r#"<?php
interface FocusedMetadataInterface {}
trait FocusedMetadataTrait {}
class FocusedMetadataBase {}
class FocusedMetadataOwner extends FocusedMetadataBase implements FocusedMetadataInterface {
    use FocusedMetadataTrait;
    public int $value = 9;
    public function method(): void {}
    public function __invoke(): void {}
    public function __destruct() { echo "D|"; }
}
function inspectFocusedEvalMetadataOwners(FocusedMetadataOwner $object, string $source): void {
    eval($source);
    echo $object->value, "|";
}
$source = '$valid = [$object, "method"];
$invalid = [$object, "missing"];
$closure = function(): void {};
echo is_callable($valid) ? "V" : "v";
echo is_callable($invalid) ? "bad" : "N";
echo is_callable($closure) ? "C" : "c";
echo is_callable($object) ? "I:" : "i:";
$parents = class_parents($object);
$interfaces = class_implements($object);
$traits = class_uses($object);
$vars = get_object_vars($object);
echo count($parents), ":", count($interfaces), ":", count($traits), ":", $vars["value"], "|";
unset($valid, $invalid, $closure, $parents, $interfaces, $traits, $vars); // ' . $argc;
for ($i = 0; $i < 3; $i++) {
    $object = new FocusedMetadataOwner();
    inspectFocusedEvalMetadataOwners($object, $source);
    unset($object);
}
unset($source);
"#;
    let expected = "VNCI:1:1:1:9|9|D|".repeat(3);
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// A native exception thrown inside an eval catch must execute, and may be overridden by, finally.
#[test]
fn test_core_eval_finally_runs_after_native_throw_from_catch() {
    let source = r#"<?php
function throwFromEvalCatch(): void { throw new Exception("second"); }
$source = 'try {
    try { throw new Exception("first"); }
    catch (Exception $first) { throwFromEvalCatch(); }
    finally { echo "finally|"; }
} catch (Exception $second) { echo $second->getMessage(), "|"; } // ' . $argc;
eval($source);
$override = 'try { throw new Exception("first"); }
catch (Exception $first) { throwFromEvalCatch(); }
finally { echo "override|"; return 7; } // ' . $argc;
echo eval($override);
"#;
    assert_eq!(compile_and_run(source), "finally|second|override|7");
}

/// Eval rethrows retain their exception while catch rebinding and finally remove the source owner.
#[test]
fn test_core_eval_rethrow_survives_same_binding_catch_and_finally_unset() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function catchEvalRethrowOwner(string $source): void {
    try { eval($source); }
    catch (Exception $caught) { echo $caught->getMessage(), "|"; unset($caught); }
}
$source = '$error = new Exception("kept");
try {
    try { throw $error; }
    catch (Throwable $error) { throw $error; }
} finally { unset($error); } // ' . $argc;
catchEvalRethrowOwner($source);
catchEvalRethrowOwner($source);
unset($source);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "kept|kept|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

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
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(r#"<?php
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
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}\nGenerated user assembly:\n{}", out.stderr, asm);
}

/// A native caller retains eval's boxed previous link after releasing the outer exception.
#[test]
fn test_core_eval_throwable_previous_survives_outer_release() {
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(r#"<?php
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
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}\nGenerated user assembly:\n{}", out.stderr, asm);
}

/// Native exceptions caught and rethrown by opaque eval balance both ownership transfers.
#[test]
fn test_core_throwable_native_eval_round_trip_releases_both_box_owners() {
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(r#"<?php
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
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}\nGenerated user assembly:\n{}", out.stderr, asm);
}

/// Catching a native exception entirely within eval must retire the exported box and raw object.
#[test]
fn test_core_native_throwable_caught_in_eval_releases_its_exported_owner() {
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(r#"<?php
function throwNativeCaughtOwner(): void { throw new RuntimeException("inner", 13); }
function catchNativeOwnerInsideEval(string $source): void { eval($source); }
$source = 'try { throwNativeCaughtOwner(); }
catch (RuntimeException $inner) { echo $inner->getMessage(), ":", $inner->getCode(), "|"; unset($inner); } // ' . $argc;
catchNativeOwnerInsideEval($source);
catchNativeOwnerInsideEval($source);
unset($source);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "inner:13|inner:13|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}\nGenerated user assembly:\n{}", out.stderr, asm);
}

/// Rethrowing the same native exception through eval must not leave the catch binding as an owner.
#[test]
fn test_core_native_throwable_rethrown_by_eval_releases_the_catch_binding() {
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(r#"<?php
function throwNativeRethrownOwner(): void { throw new RuntimeException("inner", 13); }
function catchRethrownEvalOwner(string $source): void {
    try { eval($source); }
    catch (RuntimeException $caught) { echo $caught->getMessage(), ":", $caught->getCode(), "|"; unset($caught); }
}
$source = 'try { throwNativeRethrownOwner(); } catch (RuntimeException $inner) { throw $inner; } // ' . $argc;
catchRethrownEvalOwner($source);
catchRethrownEvalOwner($source);
unset($source);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "inner:13|inner:13|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}\nGenerated user assembly:\n{}", out.stderr, asm);
}

/// Releasing an eval wrapper must retire its native previous exception without reading getPrevious.
#[test]
fn test_core_native_throwable_wrapped_by_eval_releases_unread_previous() {
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(r#"<?php
function throwNativeWrappedOwner(): void { throw new RuntimeException("inner", 13); }
function catchWrappedEvalOwner(string $source): void {
    try { eval($source); }
    catch (Exception $caught) { echo $caught->getMessage(), ":", $caught->getCode(), "|"; unset($caught); }
}
$source = 'try { throwNativeWrappedOwner(); }
catch (RuntimeException $inner) { throw new Exception("outer", 7, $inner); } // ' . $argc;
catchWrappedEvalOwner($source);
catchWrappedEvalOwner($source);
unset($source);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "outer:7|outer:7|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}\nGenerated user assembly:\n{}", out.stderr, asm);
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
