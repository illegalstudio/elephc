//! Purpose:
//! Covers authorized native property unsets and their observable lifetime effects.
//!
//! Called from:
//! - The codegen test harness through the runtime GC test group.
//!
//! Key details:
//! - Opaque eval keeps native declarations outside Magician's declaration table.
//! - The same operations cover direct native objects and eval-declared subclasses.

use crate::support::*;

/// Magic native unsetters remain callable for inaccessible and absent properties without recursive reentry.
#[test]
fn test_core_eval_native_unset_magic_fallback_and_exception_guard() {
    let source = r#"<?php
class NativeMagicUnset {
    private int $hidden = 1;
    public function __unset(string $name): void {
        if ($name === "throwing") { throw new Exception("stop"); }
        eval('unset($this->{$name}); unset($this->{$name});');
        echo $name, ":";
    }
}
$source = 'class EvalMagicUnset extends NativeMagicUnset {}
foreach ([new NativeMagicUnset(), new EvalMagicUnset()] as $object) {
    unset($object->hidden); unset($object->missing);
    try { unset($object->throwing); } catch (Exception $error) { echo "caught:"; }
    try { unset($object->throwing); } catch (Exception $error) { echo "again|"; }
}' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "hidden:missing:caught:again|hidden:missing:caught:again|");
}

/// Repeated native and inherited unsets release nested values and do not accumulate metadata owners.
#[test]
fn test_core_eval_native_unset_releases_payload_and_metadata_owners() {
    for class in ["NativeUnsetOwners", "EvalUnsetOwners"] {
        super::core_builtins::assert_core_eval_collection_cleanup_with_native(
            "class NativeUnsetOwners { public mixed $payload; }",
            &format!("class EvalUnsetOwners extends NativeUnsetOwners {{}} $object = new {class}();"),
            "$object->payload = [1, [2, 3]]; unset($object->payload); unset($object->payload);",
        );
    }
}

/// A native lexical private slot is unset without clearing a same-named public child property.
#[test]
fn test_core_eval_native_unset_selects_lexical_private_storage() {
    let source = r#"<?php
class NativeUnsetPrivate {
    private int $value = 3;
    public function clear(): void { eval('unset($this->value);'); }
    public function initialized(): bool {
        return (new ReflectionProperty(NativeUnsetPrivate::class, "value"))->isInitialized($this);
    }
}
class NativeUnsetPrivateChild extends NativeUnsetPrivate { public int $value = 9; }
$source = 'class EvalUnsetPrivateChild extends NativeUnsetPrivate { public int $value = 9; }
foreach ([new NativeUnsetPrivateChild(), new EvalUnsetPrivateChild()] as $object) {
    $object->clear(); echo $object->initialized() ? "bad" : "private", ":", $object->value, "|";
}' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "private:9|private:9|");
}

/// Native and inherited typed slots become uninitialized, accept reassignment, and survive repeated unset.
#[test]
fn test_core_eval_native_unset_marks_physical_slots_uninitialized() {
    let source = r#"<?php
class NativeUnsetMatrix {
    public int $number = 7;
    public ?string $text = "before";
    public mixed $nested = [1, [2, 3]];
}
$source = 'class EvalUnsetMatrix extends NativeUnsetMatrix {}
foreach ([new NativeUnsetMatrix(), new EvalUnsetMatrix()] as $object) {
    $number = new ReflectionProperty("NativeUnsetMatrix", "number");
    $text = new ReflectionProperty("NativeUnsetMatrix", "text");
    $nested = new ReflectionProperty("NativeUnsetMatrix", "nested");
    unset($object->number); unset($object->text); unset($object->nested);
    unset($object->number); unset($object->text); unset($object->nested);
    echo $number->isInitialized($object) ? "bad" : "N";
    echo $text->isInitialized($object) ? "bad" : "T";
    echo $nested->isInitialized($object) ? "bad" : "M";
    $object->number = 9; $object->text = null; $object->nested = [4];
    echo ":", $object->number, ":", $text->isInitialized($object), ":", $object->nested[0], "|";
}' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "NTM:9:1:4|NTM:9:1:4|");
}

/// Native private, asymmetric-set, readonly, and hooked properties reject unauthorized unsets.
#[test]
fn test_core_eval_native_unset_preserves_property_guards() {
    let source = r#"<?php
class NativeUnsetGuards {
    private int $hidden = 1;
    public private(set) int $guarded = 2;
    public readonly int $locked;
    public int $hooked = 4 { set { $this->hooked = $value; } }
    public function __construct() { $this->locked = 3; }
}
$source = 'class EvalUnsetGuards extends NativeUnsetGuards {}
foreach ([new NativeUnsetGuards(), new EvalUnsetGuards()] as $object) {
    foreach (["hidden", "guarded", "locked", "hooked"] as $name) {
        try { unset($object->{$name}); echo "bad"; }
        catch (Error $error) { echo $name, ":"; }
    }
    echo $object->guarded, ":", $object->locked, ":", $object->hooked, "|";
}' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "hidden:guarded:locked:hooked:2:3:4|hidden:guarded:locked:hooked:2:3:4|");
}

/// Eval-declared readonly properties cannot be unset after initialization, even inside their constructor.
#[test]
fn test_core_eval_unset_rejects_initialized_readonly_and_hooked_properties() {
    let source = r#"<?php
$source = 'class EvalUnsetReadonly {
    public readonly int $value;
    public function __construct() {
        $this->value = 8;
        try { unset($this->value); echo "bad"; }
        catch (Error $error) { echo "readonly:"; }
    }
}
class EvalUnsetHooked {
    public int $value = 5 { get => $this->value; }
}
$readonly = new EvalUnsetReadonly(); echo $readonly->value, "|";
$hooked = new EvalUnsetHooked();
try { unset($hooked->value); echo "bad"; }
catch (Error $error) { echo "hooked:"; }
echo $hooked->value;' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "readonly:8|hooked:5");
}

/// Uninitialized native readonly properties require their declaring scope even before a first write.
#[test]
fn test_core_eval_native_unset_uninitialized_readonly_requires_scope() {
    let source = r#"<?php
class NativeUnsetUninitializedReadonly {
    public readonly int $value;
    public function clear(): void { eval('unset($this->value);'); }
}
$source = 'class EvalUnsetUninitializedReadonly extends NativeUnsetUninitializedReadonly {}
foreach ([new NativeUnsetUninitializedReadonly(), new EvalUnsetUninitializedReadonly()] as $object) {
    try { unset($object->value); echo "bad"; }
    catch (Error $error) { echo "guarded:"; }
    $object->clear();
    $property = new ReflectionProperty("NativeUnsetUninitializedReadonly", "value");
    echo $property->isInitialized($object) ? "bad" : "uninitialized|";
}' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "guarded:uninitialized|guarded:uninitialized|");
}
