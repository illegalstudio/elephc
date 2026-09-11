//! Purpose:
//! Verifies ownership and cycle traversal for receivers retained by eval closures.
//!
//! Called from:
//! - The native codegen regression harness on executable CI targets.
//!
//! Key details:
//! - Opaque source uses the live Magician/runtime bridge.
//! - Destructors prove release timing, cycle traversal, and throwable cleanup.

use crate::support::*;

/// Native and eval method receivers survive variable removal and release with their closures.
#[test]
fn test_core_eval_closure_receivers_survive_collection_and_release() {
    let source = r#"<?php
function collectEvalClosureReceiverCycles(): int { return 0; }
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
collectEvalClosureReceiverCycles();
echo $nativeCallback(), ":", $dynamicCallback(), "|";
unset($nativeCallback); unset($dynamicCallback);
collectEvalClosureReceiverCycles(); echo "|done";' . ' // ' . $argc;
eval($source);
"#;
    assert_heap_clean_output(source, "native:eval|NE|done");
}

/// An external closure root preserves its receiver cycle until both roots are removed.
#[test]
fn test_core_eval_closure_receiver_cycle_is_traced_and_collected() {
    let source = r#"<?php
function collectEvalClosureReceiverCycles(): int { return 0; }
$source = 'class EvalClosureCycle {
    public mixed $callback;
    public function read(): string { return "live"; }
    public function __destruct() { echo "D"; }
}
$object = new EvalClosureCycle();
$callback = $object->read(...);
$object->callback = $callback;
unset($object);
collectEvalClosureReceiverCycles(); echo $callback(), "|";
unset($callback);
collectEvalClosureReceiverCycles(); echo "|done";' . ' // ' . $argc;
eval($source);
"#;
    assert_heap_clean_output(source, "live|D|done");
}

/// Receiver destructor throws remain catchable and do not retain closure metadata.
#[test]
fn test_core_eval_closure_receiver_throw_cleans_owner_metadata() {
    let source = r#"<?php
function collectEvalClosureReceiverCycles(): int { return 0; }
$source = 'class EvalThrowingClosureReceiver {
    public function read(): string { return "live"; }
    public function __destruct() { throw new RuntimeException("receiver"); }
}
$object = new EvalThrowingClosureReceiver();
$callback = $object->read(...);
unset($object);
try { unset($callback); collectEvalClosureReceiverCycles(); }
catch (Throwable $error) { echo get_class($error), ":", $error->getMessage(); }
echo "|done";' . ' // ' . $argc;
eval($source);
"#;
    assert_heap_clean_output(source, "RuntimeException:receiver|done");
}

/// A retained method receiver releases with its first-class callable.
#[test]
fn test_core_eval_method_callable_receiver_temporary_releases() {
    assert_receiver_temporary_cleanup("$object->read(...)");
}

/// A retained invokable receiver releases with its first-class callable.
#[test]
fn test_core_eval_invokable_callable_receiver_temporary_releases() {
    assert_receiver_temporary_cleanup("$object(...)");
}

/// A constructor-produced receiver transfers directly into its method callable.
#[test]
fn test_core_eval_constructed_method_callable_receiver_temporary_releases() {
    assert_receiver_temporary_cleanup("(new EvalTemporaryReceiver())->read(...)");
}

/// Closure::fromCallable releases the array and method cells around its receiver.
#[test]
fn test_core_eval_from_callable_method_receiver_temporary_releases() {
    assert_receiver_temporary_cleanup("Closure::fromCallable([$object, \"read\"])");
}

/// Closure::fromCallable releases the original invokable receiver argument.
#[test]
fn test_core_eval_from_callable_invokable_receiver_temporary_releases() {
    assert_receiver_temporary_cleanup("Closure::fromCallable($object)");
}

/// Nested first-class construction transfers one receiver owner through Closure::fromCallable.
#[test]
fn test_core_eval_from_callable_constructed_receiver_temporary_releases() {
    assert_receiver_temporary_cleanup(
        "Closure::fromCallable((new EvalTemporaryReceiver())->read(...))",
    );
}

/// Runs one output fixture with heap guards and requires complete cleanup.
fn assert_heap_clean_output(source: &str, expected: &str) {
    let directory = make_cli_test_dir("eval_closure_receiver_gc");
    let (assembly, runtime, libraries) =
        compile_source_to_asm_with_options(source, &directory, 8_388_608, false, true);
    let patched = super::mbstring_capture_hash::replace_function(
        &assembly,
        "collectEvalClosureReceiverCycles",
        &super::mbstring_capture_hash::reference_begin::invoke::deferred::collect_shim(),
    );
    let output = assemble_and_run_capture(
        &patched,
        &runtime_obj_for_asm(&runtime),
        &directory,
        &libraries,
        &default_link_paths(),
        &[],
    );
    let _ = std::fs::remove_dir_all(directory);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected);
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        output.stderr
    );
}

/// Compares one and five closure constructions so retained receiver edges cannot accumulate.
fn assert_receiver_temporary_cleanup(callable: &str) {
    let outstanding = |iterations| {
        let body = format!(
            "$object = new EvalTemporaryReceiver(); $callback = {callable};
             unset($object); unset($callback);"
        )
        .repeat(iterations);
        let source = format!(
            r#"<?php
$source = 'class EvalTemporaryReceiver {{
    public function read(): string {{ return "value"; }}
    public function __invoke(): string {{ return "value"; }}
}}
{body}
return 42;' . ' // ' . $argc;
echo eval($source);
"#
        );
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "42");
        let (allocations, frees) = parse_gc_stats(&output.stderr);
        (allocations, allocations as i128 - frees as i128)
    };
    let (once_allocated, once_live) = outstanding(1);
    let (repeated_allocated, repeated_live) = outstanding(5);
    assert!(repeated_allocated > once_allocated);
    assert_eq!(
        repeated_live, once_live,
        "eval closure retained per-call receiver storage: {callable}"
    );
}
