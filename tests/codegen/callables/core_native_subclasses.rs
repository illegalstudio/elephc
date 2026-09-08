//! Purpose:
//! Verifies opaque eval subclasses keep native parent fields and extra property storage coherent.
//!
//! Called from:
//! - The codegen callable integration suite.
//!
//! Key details:
//! - Sources remain opaque until runtime and exercise clone, visibility, COW, and GC ownership.

use crate::support::*;

/// Unset eval overrides invalidate the native slot, and later writes initialize both views again.
#[test]
fn test_core_eval_native_typed_property_unset_keeps_both_views_uninitialized() {
    let source = r#"<?php
class NativeTypedUnsetParent {
    public int $value = 3;
    protected string $label = "parent";
    public function nativeValue(): int { return $this->value; }
    public function nativeLabel(): string { return $this->label; }
}
$source = 'class EvalTypedUnsetChild extends NativeTypedUnsetParent {
    public int $value = 9;
    protected string $label = "child";
    public function clearLabel(): void { unset($this->label); }
}
$child = new EvalTypedUnsetChild();
echo $child->nativeValue(), ":", $child->nativeLabel(), "|";
unset($child->value);
unset($child->value);
try { echo $child->value; } catch (Error $error) { echo "eval-unset|"; }
try { echo $child->nativeValue(); } catch (Error $error) { echo "native-unset|"; }
$child->clearLabel();
try { echo $child->nativeLabel(); } catch (Error $error) { echo "label-unset|"; }
$child->value = 15;
echo $child->value, ":", $child->nativeValue();' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "9:child|eval-unset|native-unset|label-unset|15:15");
}

/// Same-named eval fields never overwrite private native parent fields or expose private access.
#[test]
fn test_core_eval_native_private_parent_property_has_distinct_child_storage() {
    let source = r#"<?php
class NativePrivateParent {
    private int $value = 3;
    public function nativeValue(): int { return $this->value; }
}
$source = 'class EvalPublicPrivateShadow extends NativePrivateParent { public int $value = 9; }
class EvalProtectedPrivateShadow extends NativePrivateParent {
    protected int $value = 11;
    public function childValue(): int { return $this->value; }
}
$public = new EvalPublicPrivateShadow();
echo $public->value, ":", $public->nativeValue(), "|";
$public->value = 15;
$copy = clone $public;
$copy->value = 17;
echo $public->value, ":", $copy->value, ":", $copy->nativeValue(), "|";
$vars = get_mangled_object_vars($public);
echo $vars["value"], ":", $vars["\0NativePrivateParent\0value"], "|";
$protected = new EvalProtectedPrivateShadow();
echo $protected->childValue(), ":", $protected->nativeValue(), "|";
try { echo $protected->value; } catch (Error $error) { echo "protected|"; }
$parent = new NativePrivateParent();
try { echo $parent->value; } catch (Error $error) { echo "private"; }' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "9:3|15:17:3|15:3|11:3|protected|private");
}

/// Eval overrides initialize and share protected native storage without exposing protected reads.
#[test]
fn test_core_eval_native_protected_property_overrides_use_declaring_scope() {
    let source = r#"<?php
class NativeProtectedParent {
    protected int $guard = 3;
    public function nativeGuard(): int { return $this->guard; }
}
$source = 'class EvalProtectedChild extends NativeProtectedParent {
    protected int $guard = 9;
    public function change(): void { $this->guard = 11; }
    public function inspect(): int { return $this->guard; }
}
class EvalPublicChild extends NativeProtectedParent { public int $guard = 15; }
$child = new EvalProtectedChild();
echo $child->inspect(), ":", $child->nativeGuard(), "|";
$child->change();
echo $child->inspect(), ":", $child->nativeGuard(), "|";
try { echo $child->guard; } catch (Error $error) { echo "protected|"; }
$public = new EvalPublicChild();
echo $public->guard, ":", $public->nativeGuard(), "|";
$public->guard = 17;
echo $public->nativeGuard();' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "9:9|11:11|protected|15:15|17");
}

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
