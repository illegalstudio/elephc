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
