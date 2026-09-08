//! Purpose:
//! Verifies cycle collection follows dynamic-property hashes and boxed property cells.
//!
//! Called from:
//! - The codegen runtime GC integration suite.
//!
//! Key details:
//! - Both incoming-edge counting and reachability must include the dynamic-property tail.
//! - Native and eval-declared destructors make incorrect collection observable.
//! - Mixed property slots own cell pointers; their high words are not runtime tags.

use crate::support::*;

/// Cyclic peer data remains readable until all native destructors finish, with nested GC suppressed.
#[test]
fn test_core_gc_destructors_keep_native_peer_data_alive() {
    let source = r#"<?php
class PinnedNativePeer {
    public mixed $peer = null;
    public string $name;
    public function __construct(string $name) { $this->name = $name; }
    public function __destruct() {
        echo $this->name, ":", $this->peer->name, ":";
        echo gc_collect_cycles() === 0 ? "nested-safe|" : "bad|";
    }
}
gc_disable();
$left = new PinnedNativePeer("A");
$right = new PinnedNativePeer("B");
$left->peer = $right;
$right->peer = $left;
unset($left, $right);
echo gc_collect_cycles() > 0 ? "collected:" : "missed:";
echo gc_status()["running"] ? "busy:" : "idle:";
echo gc_collect_cycles() === 0 ? "empty" : "bad";
"#;
    let output = compile_and_run(source);
    assert_eq!(output.matches("A:B:nested-safe|").count(), 1, "{output}");
    assert_eq!(output.matches("B:A:nested-safe|").count(), 1, "{output}");
    assert!(output.ends_with("collected:idle:empty"), "{output}");
}

/// Eval destructor resurrection preserves object metadata and does not run the destructor twice.
#[test]
fn test_core_gc_eval_destructor_resurrection_preserves_metadata() {
    let source = r#"<?php
$source = 'class PinnedEvalRoot { public static $saved = null; }
class PinnedEvalCycle {
    public function __construct($name) { $this->name = $name; }
    public function __destruct() {
        echo "drop:", $this->name, ":";
        PinnedEvalRoot::$saved = $this;
    }
}
gc_disable();
$object = new PinnedEvalCycle("alive");
$object->self = $object;
unset($object);
gc_collect_cycles();
echo get_class(PinnedEvalRoot::$saved), ":", PinnedEvalRoot::$saved->name, ":";
PinnedEvalRoot::$saved = null;
echo gc_collect_cycles() > 0 ? "collected:" : "missed:";
echo gc_collect_cycles() === 0 ? "empty" : "bad";' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "drop:alive:PinnedEvalCycle:alive:collected:empty");
}

/// Destructor mutation of a cyclic graph must be recounted before the collector frees its nodes.
#[test]
fn test_core_gc_eval_destructor_can_detach_cyclic_edges() {
    let source = r#"<?php
$source = 'class PinnedEvalMutation {
    public function __construct($name) { $this->name = $name; }
    public function __destruct() {
        echo $this->name, ":";
        $this->self = null;
        echo gc_collect_cycles() === 0 ? "nested-safe:" : "bad:";
    }
}
gc_disable();
$object = new PinnedEvalMutation("intact");
$object->self = $object;
unset($object);
echo gc_collect_cycles() > 0 ? "collected:" : "missed:";
echo gc_collect_cycles() === 0 ? "empty" : "bad";' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "intact:nested-safe:collected:empty");
}

/// A live stdClass root protects a cyclic eval object until the root itself is released.
#[test]
fn test_core_gc_dynamic_property_hash_preserves_rooted_eval_cycles() {
    let source = r#"<?php
$source = 'class DynamicCycleChild {
    public function __destruct() { echo "drop:"; }
}
gc_disable();
$root = new stdClass();
$child = new DynamicCycleChild();
$child->self = $child;
$root->child = $child;
unset($child);
gc_collect_cycles();
echo "kept:";
unset($root);
gc_collect_cycles();
echo "done";' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "kept:drop:done");
}

/// A native dynamic-property tail contributes an internal edge, not a phantom external root.
#[test]
fn test_core_gc_dynamic_property_hash_collects_native_cycles() {
    let source = r#"<?php
#[AllowDynamicProperties]
class DynamicNativeCycle {
    public int $marker = 7;
    public function __destruct() { echo "drop:", $this->marker, ":"; }
}
gc_disable();
$box = new DynamicNativeCycle();
$box->self = $box;
unset($box);
echo gc_collect_cycles() > 0 ? "collected" : "missed";
"#;
    assert_eq!(compile_and_run(source), "drop:7:collected");
}

/// A rooted native Mixed property cycle survives until its last external owner is removed.
#[test]
fn test_core_gc_boxed_property_collects_native_cycle_after_root_release() {
    let source = r#"<?php
class NativeBoxedCycle {
    public mixed $link = null;
    public function __destruct() { echo "drop:"; }
}
gc_disable();
$box = new NativeBoxedCycle();
$box->link = $box;
gc_collect_cycles();
echo "kept:";
unset($box);
echo gc_collect_cycles() > 0 ? "collected" : "missed";
"#;
    assert_eq!(compile_and_run(source), "kept:drop:collected");
}

/// Eval writes to AOT Mixed slots use the same boxed-cell graph as ordinary native writes.
#[test]
fn test_core_gc_boxed_property_collects_eval_written_native_cycle() {
    let source = r#"<?php
class EvalWrittenBoxedCycle {
    public mixed $link = null;
    public function __destruct() { echo "drop:"; }
}
$source = 'gc_disable();
$box = new EvalWrittenBoxedCycle();
$box->link = $box;
gc_collect_cycles();
echo "kept:";
unset($box);
echo gc_collect_cycles() > 0 ? "collected" : "missed";' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "kept:drop:collected");
}
