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
