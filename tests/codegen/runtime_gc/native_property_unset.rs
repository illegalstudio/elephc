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

/// Eval-declared throwing destructors release their object and fields before the catch resumes.
#[test]
fn test_core_eval_dynamic_destructor_throw_consumes_last_owner() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function releaseDynamicEvalObjects(string $source): void { eval($source); }
$source = 'class DynamicReleaseThrow {
    public string $buffer;
    public function __construct() { $this->buffer = str_repeat("x", 48); }
    public function __destruct() { throw new RuntimeException("dynamic"); }
}
$caught = 0;
for ($i = 0; $i < 3; $i++) {
    $value = new DynamicReleaseThrow();
    try { unset($value); }
    catch (RuntimeException $error) { $caught++; unset($error); }
}
echo $caught;' . ' // ' . $argc;
releaseDynamicEvalObjects($source);
unset($source);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Scope retirement finishes all Rust-held owners before an escaping destructor throw reaches PHP.
#[test]
fn test_core_eval_scope_teardown_finishes_all_throwing_children_before_escape() {
    let source = r#"<?php
class NativeScopeThrowChild {
    public static int $calls = 0;
    public function __destruct() { self::$calls++; throw new RuntimeException("scope"); }
}
function leaveOwnedEvalScope(string $source): void { eval($source); }
$source = '$first = new NativeScopeThrowChild(); $second = new NativeScopeThrowChild(); // ' . $argc;
try { leaveOwnedEvalScope($source); }
catch (RuntimeException $error) {
    $previous = $error->getPrevious();
    echo $previous !== null && $previous->getMessage() === "scope" ? "chain:" : "lost:";
    echo NativeScopeThrowChild::$calls;
}
"#;
    assert_eq!(compile_and_run(source), "chain:2");
}

/// Releasing eval arrays contains native child destructor throws before returning through Rust.
#[test]
fn test_core_eval_array_release_contains_native_destructor_exceptions() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class EvalReleaseNativeChild {
    public string $buffer;
    public function __construct() { $this->buffer = str_repeat("x", 48); }
    public function __destruct() { throw new RuntimeException("child"); }
}
function exerciseEvalArrayRelease(string $source): void {
    try { throw new Exception("outer"); }
    catch (Exception $outer) {
        eval($source);
        echo $outer->getMessage(), "|";
    }
}
$source = '$caught = 0;
for ($i = 0; $i < 3; $i++) {
    $children = [new EvalReleaseNativeChild(), new EvalReleaseNativeChild()];
    try { unset($children); }
    catch (RuntimeException $error) {
        if ($error->getMessage() === "child" && $error->getPrevious() !== null) { $caught++; }
        unset($error);
    }
}
echo $caught, ":";' . ' // ' . $argc;
exerciseEvalArrayRelease($source);
unset($source);
echo gc_status()["protected"] ? "protected" : "ready";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3:outer|ready", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A destructor thrown during native-slot unset is caught inside eval without losing outer native state.
#[test]
fn test_core_eval_native_unset_contains_destructor_exceptions_and_restores_scope() {
    let source = r#"<?php
class NativeUnsetThrowChild {
    public string $buffer;
    public function __construct() { $this->buffer = str_repeat("x", 48); }
    public function __destruct() { throw new RuntimeException("inner"); }
}
class NativeUnsetThrowHolder { public NativeUnsetThrowChild $child; }
function exerciseNativeUnsetThrow(string $source): void {
    $holder = new NativeUnsetThrowHolder();
    try { throw new Exception("outer"); }
    catch (Exception $outer) {
        eval($source);
        echo $outer->getMessage(), "|";
        echo eval('return "again";');
    }
}
$source = '$count = 0;
for ($i = 0; $i < 3; $i++) {
    $holder->child = new NativeUnsetThrowChild();
    try { unset($holder->child); }
    catch (RuntimeException $error) {
        if ($error->getMessage() === "inner" && !isset($holder->child)) { $count++; }
        unset($error);
    }
}
echo $count, ":";' . ' // ' . $argc;
exerciseNativeUnsetThrow($source);
"#;
    assert_eq!(compile_and_run(source), "3:outer|again");
}

/// Repeated destructor throws free native children and temporary exception boxes after eval catches them.
#[test]
fn test_core_eval_native_unset_destructor_exceptions_do_not_accumulate_owners() {
    super::core_builtins::assert_core_eval_collection_cleanup_with_native(
        "class NativeUnsetThrowOwner {
            public string $buffer;
            public function __construct() { $this->buffer = str_repeat(\"x\", 48); }
            public function __destruct() { throw new RuntimeException(\"stop\"); }
         }
         class NativeUnsetThrowBox { public NativeUnsetThrowOwner $child; }",
        "$holder = new NativeUnsetThrowBox();",
        "$holder->child = new NativeUnsetThrowOwner();
         try { unset($holder->child); } catch (RuntimeException $error) { unset($error); }",
    );
}

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

/// Repeated native unsets release nested values and do not accumulate metadata owners.
#[test]
fn test_core_eval_native_unset_releases_payload_and_metadata_owners() {
    assert_unset_payload_cleanup("NativeUnsetOwners");
}

/// Inherited physical-slot unsets release both payload and eval metadata owners.
#[test]
fn test_core_eval_inherited_unset_releases_payload_and_metadata_owners() {
    assert_unset_payload_cleanup("EvalUnsetOwners");
}

/// Measures one allocation path separately so the native and inherited CI cases remain bounded.
fn assert_unset_payload_cleanup(class: &str) {
    super::core_builtins::assert_core_eval_collection_cleanup_with_native(
        "class NativeUnsetOwners { public mixed $payload; }",
        &format!("class EvalUnsetOwners extends NativeUnsetOwners {{}} $object = new {class}();"),
        "$object->payload = [1, [2, 3]]; unset($object->payload); unset($object->payload);",
    );
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
