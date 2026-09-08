//! Purpose:
//! Verifies cycle collection follows the hash owned by dynamic-property object layouts.
//!
//! Called from:
//! - The codegen runtime GC integration suite.
//!
//! Key details:
//! - Both incoming-edge counting and reachability must include the dynamic-property tail.
//! - Native and eval-declared destructors make incorrect collection observable.

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
