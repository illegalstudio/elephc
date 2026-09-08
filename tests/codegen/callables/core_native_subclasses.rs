//! Purpose:
//! Verifies opaque eval subclasses keep native parent fields and extra property storage coherent.
//!
//! Called from:
//! - The codegen callable integration suite.
//!
//! Key details:
//! - Sources remain opaque until runtime and exercise clone, visibility, COW, and GC ownership.

use crate::support::*;

/// Extra eval fields coexist with native overrides, protected fields, and independent cloned arrays.
#[test]
fn test_core_eval_native_subclass_property_storage_and_clone() {
    let source = r#"<?php
class NativeStorageParent {
    public int $shared = 2;
    protected int $guard = 3;
    public function nativeShared(): int { return $this->shared; }
}
$source = 'class EvalStorageChild extends NativeStorageParent {
    public int $shared = 20;
    public array $items = [1, 2];
    private string $secret = "child";
    public function inspect(): void {
        echo $this->guard, ":", $this->secret, ":", $this->nativeShared();
    }
}
$child = new EvalStorageChild();
$copy = clone $child;
$copy->items[0] = 9;
$copy->shared = 30;
$child->inspect(); echo "|"; $copy->inspect();
echo "|", implode(",", $child->items), ":", implode(",", $copy->items);
unset($child, $copy);' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "3:child:20|3:child:30|1,2:9,2");
}

/// An eval subclass's extra property hash participates in native cycle collection.
#[test]
fn test_core_eval_native_subclass_extra_property_cycle() {
    let source = r#"<?php
class NativeCycleStorageParent { public int $marker = 7; }
$source = 'class EvalCycleStorageChild extends NativeCycleStorageParent {
    public mixed $link = null;
    public function __destruct() { echo "drop:", $this->marker, ":"; }
}
gc_disable();
$child = new EvalCycleStorageChild();
$child->link = $child;
gc_collect_cycles(); echo "kept:";
unset($child);
echo gc_collect_cycles() > 0 ? "collected" : "missed";' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "kept:drop:7:collected");
}
