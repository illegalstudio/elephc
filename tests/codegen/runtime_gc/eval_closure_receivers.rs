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

/// Interpreted `$compiledClosure->bindTo($obj)` hands back a box that owns the bound descriptor
/// alone: dropping the result releases the descriptor and the bound `$this`, so the receiver's
/// destructor runs at the `unset()`, as in php. The bridge boxed the fresh descriptor with the
/// retaining boxer and never dropped its construction reference, leaking descriptor and receiver.
#[test]
fn test_eval_bind_to_of_a_compiled_closure_releases_the_bound_receiver() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class BoundReceiver {
    public $n;
    function __construct($n) { $this->n = $n; }
    function __destruct() { echo "destruct {$this->n}\n"; }
}
function rebindThroughEval(\Closure $f, object $o, string $code) { return eval($code); }
$f = function () { return $this->n; };
$code = $argc > 0 ? 'return $f->bindTo($o);' : '';
for ($i = 1; $i <= 3; $i++) {
    $g = rebindThroughEval($f, new BoundReceiver($i), $code);
    echo $g(), "\n";
    unset($g);
    echo "after $i\n";
}
echo "end\n";
"#);
    assert_eq!(
        out.stdout,
        "1\ndestruct 1\nafter 1\n2\ndestruct 2\nafter 2\n3\ndestruct 3\nafter 3\nend\n",
        "stderr: {}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Interpreted code storing a closure into a compiled `\Closure` property releases the
/// descriptor the slot held before. The callable arm of the eval property setter published the
/// converted descriptor with a bare store, so every reassignment leaked the previous one.
#[test]
fn test_eval_closure_property_reassignment_releases_the_previous_descriptor() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ClosureHolder { public \Closure $h; }
function setHolderThroughEval(ClosureHolder $o, string $code) { eval($code); }
$code = $argc > 0 ? '$o->h = function () { return 1; };' : '';
for ($i = 0; $i < 3; $i++) {
    $o = new ClosureHolder();
    setHolderThroughEval($o, $code);
    setHolderThroughEval($o, $code);
    echo ($o->h)(), "\n";
}
unset($o);
echo "end\n";
"#);
    assert_eq!(out.stdout, "1\n1\n1\nend\n", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An assignment inside `function () use (&$x)` stores through the shared reference, which copies
/// the value, so the statement's own owner has to be released with the entry. Every assignment
/// leaked one boxed value (object, string, array) until the entry owned it.
#[test]
fn test_eval_closure_by_reference_capture_assignment_releases_the_written_value() {
    let source = r#"<?php
class ByRefCaptureOwner {
    public function __construct(public string $label) {}
    public function __destruct() { echo $this->label, "|"; }
}
$source = '$object = new ByRefCaptureOwner("old");
$callback = function() use (&$object): string { $object = new ByRefCaptureOwner("new"); return $object->label; };
echo $callback(), ":"; unset($object, $callback); echo "done|";
$text = "a"; $callback = function() use (&$text) { $text = str_repeat("z", 3); };
$callback(); unset($callback); echo $text, "|";
$list = []; $callback = function() use (&$list) { $list = [1, 2]; };
$callback(); unset($callback); echo count($list);' . ' // ' . $argc;
eval($source);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "old|new:new|done|zzz|2", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A compiled call of an eval-created closure that takes a by-reference argument stores through
/// the caller's slot, which keeps its own copy of the written value: the statement's reference
/// stays with the closure's scope entry. Treating it as borrowed leaked the string and its box
/// (the foreign by-reference closure shape behind the SimpleXML reference-argument leaks).
#[test]
fn test_eval_closure_by_reference_argument_assignment_releases_the_written_value() {
    let source = r#"<?php
function viaMixed(mixed $callback, mixed $x): void { $callback($x); echo $x, "|"; }
function viaInt(mixed $callback, int $x): void { $callback($x); }
eval('$foreignReference = function (&$value): void { $value = "ref"; };');
viaMixed($foreignReference, 5);
viaInt($foreignReference, 5);
echo "end";
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "ref|end", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
