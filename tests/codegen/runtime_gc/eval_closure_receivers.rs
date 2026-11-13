//! Purpose:
//! Verifies ownership and cycle traversal for receivers retained by eval closures.
//!
//! Called from:
//! - The native codegen regression harness on executable CI targets.
//!
//! Key details:
//! - Opaque source requires the live Magician/runtime bridge, not FakeOps.
//! - Destructors prove release timing; explicit collection probes both rooted and cyclic graphs.

use crate::support::*;

/// Captures belong to each Closure object, including bound copies, reference cells and cycles.
#[test]
fn test_core_eval_closure_capture_edges_release_bundle() {
    let definitions = r#"class CapturedEvalOwner {
    public function __construct(public string $label, public bool $throws = false) {}
    public function __destruct() {
        echo $this->label, "|";
        if ($this->throws) { throw new RuntimeException("capture"); }
    }
}"#;
    let body = r#"if ($argc === 1) {
    $object = new CapturedEvalOwner("one");
    $callback = function() use ($object): string { return $object->label; };
    unset($object); echo $callback(), ":"; unset($callback); echo "done";
} elseif ($argc === 2) {
    $object = new CapturedEvalOwner("two");
    $callback = function() use ($object): string { return $object->label; };
    $copy = $callback->bindTo(null);
    unset($callback, $object); gc_collect_cycles();
    echo $copy(), ":"; unset($copy); echo "done";
} elseif ($argc === 3) {
    $object = new CapturedEvalOwner("old");
    $callback = function() use (&$object): string {
        $object = new CapturedEvalOwner("new"); return $object->label;
    };
    echo $callback(), ":"; unset($object, $callback); echo "done";
} elseif ($argc === 4) {
    gc_disable(); $callback = null; $object = new CapturedEvalOwner("cycle");
    $callback = function() use (&$callback, $object): string { return $object->label; };
    unset($object); gc_collect_cycles(); echo $callback(), ":";
    unset($callback); gc_collect_cycles(); echo "done";
} else {
    $object = new CapturedEvalOwner("throw", true);
    $callback = function() use ($object): void {};
    unset($object);
    try { unset($callback); }
    catch (RuntimeException $error) { echo $error->getMessage(), ":"; unset($error); }
    echo "done";
}"#;
    let quoted = body.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php {definitions}\n$source = $argc > 0 ? '{quoted}' : ''; eval($source); unset($source);");
    let runs = compile_and_run_with_heap_debug_per_argv(&source, &[
        &[], &["x"], &["x", "x"], &["x", "x", "x"], &["x", "x", "x", "x"],
    ]);
    for (mode, (out, expected)) in runs.iter().zip([
        "one:one|done", "two:two|done", "old|new:new|done", "cycle:cycle|done", "throw|capture:done",
    ]).enumerate() {
        assert!(out.success, "mode={mode}\n{}", out.stderr);
        assert_eq!(out.stdout, expected, "mode={mode}\n{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "mode={mode}\n{}", out.stderr);
    }
}

/// Capture edges retain the same destruction order without EIR optimization.
#[test]
fn test_core_eval_closure_capture_edges_release_no_ir_opt_bundle() {
    without_ir_opt(test_core_eval_closure_capture_edges_release_bundle);
}

/// Native receiver throws return through Rust edge cleanup before eval catches the complete chain.
#[test]
fn test_core_eval_closure_receiver_destructor_throws_finish_native_owner_cleanup() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NativeThrowingClosureReceiver {
    public string $buffer;
    public function __construct() { $this->buffer = str_repeat("x", 48); }
    public function read(): string { return "live"; }
    public function __destruct() { throw new RuntimeException("receiver"); }
}
function releaseThrowingClosureReceivers(string $source): void { eval($source); }
$source = '$caught = 0;
for ($i = 0; $i < 3; $i++) {
    echo "new|";
    $callbacks = [(new NativeThrowingClosureReceiver())->read(...), (new NativeThrowingClosureReceiver())->read(...)];
    echo "unset|";
    try { unset($callbacks); }
    catch (RuntimeException $error) {
        echo "caught|";
        $previous = $error->getPrevious();
        if ($previous !== null && $previous->getMessage() === "receiver" && $previous->getPrevious() === null) { $caught++; }
        unset($error); unset($previous);
    }
}
echo $caught, ":", gc_status()["protected"] ? "protected" : "ready";' . ' // ' . $argc;
releaseThrowingClosureReceivers($source);
unset($source);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, format!("{}3:ready", "new|unset|caught|".repeat(3)), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Native and eval method receivers survive variable removal and are freed with their closures.
#[test]
fn test_core_eval_closure_receivers_survive_collection_and_release() {
    let source = r#"<?php
class NativeClosureReceiver {
    public function read(): string { return "native"; }
    public function __destruct() { echo "N"; }
}
$source = 'class EvalClosureReceiver {
    public function read(): string { return "eval"; }
    public function __destruct() { echo "E"; }
}
$native = new NativeClosureReceiver(); $dynamic = new EvalClosureReceiver();
$nativeCallback = $native->read(...); $dynamicCallback = $dynamic->read(...);
unset($native); unset($dynamic);
gc_collect_cycles();
echo $nativeCallback(), ":", $dynamicCallback(), "|";
unset($nativeCallback); unset($dynamicCallback);
gc_collect_cycles(); echo "|done";' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "native:eval|NE|done");
}

/// An external closure root preserves its receiver cycle, which becomes collectible after unset.
#[test]
fn test_core_eval_closure_receiver_cycle_is_traced_and_collected() {
    let source = r#"<?php
$source = 'gc_disable();
class EvalClosureCycle {
    public mixed $callback;
    public function read(): string { return "live"; }
    public function __destruct() { echo "D"; }
}
$object = new EvalClosureCycle();
$callback = $object->read(...);
$object->callback = $callback;
unset($object);
gc_collect_cycles(); echo $callback(), "|";
unset($callback);
gc_collect_cycles(); echo "|done";' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "live|D|done");
}

/// Receiver leases do not accumulate when method callable objects are discarded.
#[test]
fn test_core_eval_closure_receiver_temporaries_release_all_owners() {
    assert_receiver_cleanup("$object->read(...)");
}

/// Invokable first-class callables release their receiver leases when discarded.
#[test]
fn test_core_eval_closure_invokable_receiver_owners_release() {
    assert_receiver_cleanup("$object(...)");
}

/// A temporary new-object expression transfers its receiver owner into the closure.
#[test]
fn test_core_eval_closure_new_receiver_owners_release() {
    assert_receiver_cleanup("(new EvalTemporaryReceiver())->read(...)");
}

/// Array-form fromCallable receivers balance both normalization and closure owners.
#[test]
fn test_core_eval_closure_from_array_receiver_owners_release() {
    assert_receiver_cleanup("Closure::fromCallable([$object, \"read\"])");
}

/// Object-form fromCallable receivers release the closure's retained owner.
#[test]
fn test_core_eval_closure_from_invokable_receiver_owners_release() {
    assert_receiver_cleanup("Closure::fromCallable($object)");
}

/// Converting a temporary first-class callable does not leak either closure's receiver owner.
#[test]
fn test_core_eval_closure_from_temporary_receiver_owners_release() {
    assert_receiver_cleanup("Closure::fromCallable((new EvalTemporaryReceiver())->read(...))");
}

/// Compares one construction form with fixed setup, keeping each CI test to two compilations.
fn assert_receiver_cleanup(callable: &str) {
    super::core_builtins::assert_core_eval_collection_cleanup_with_native(
        "",
        "class EvalTemporaryReceiver {
            public function read(): string { return \"value\"; }
            public function __invoke(): string { return \"value\"; }
         }",
        &format!("$object = new EvalTemporaryReceiver(); $callback = {callable};
                  unset($object); unset($callback);"),
    );
}
