//! Purpose:
//! Verifies that reflector metadata does not retain the current static-property values.
//!
//! Called from:
//! - Supported-host executable reflection regressions in CI.
//!
//! Key details:
//! - Generic reflector accessors read live values and omit uninitialized properties.
//! - Constructing a reflector does not postpone a displaced object's destructor.

use crate::support::*;

/// A generic accessor refreshes its map while construction preserves ordinary static lifetimes.
#[test]
fn test_reflection_static_values_are_loaded_lazily_without_retaining_displaced_objects() {
    let source = r#"<?php
class LazyReflectionStaticSubject {
    public static mixed $value = null;
    public static int $uninitialized;
}

class LazyReflectionStaticTracked {
    public function __destruct() { echo 'old|'; }
}
function lazyReflectionMap(ReflectionClass $reflection): mixed { return $reflection->getStaticProperties(); }
function lazyReflectionValue(ReflectionClass $reflection): mixed { return $reflection->getStaticPropertyValue('value'); }
LazyReflectionStaticSubject::$value = new LazyReflectionStaticTracked();
$reflection = new ReflectionClass(LazyReflectionStaticSubject::class);
LazyReflectionStaticSubject::$value = 11;
echo 'assigned|';
$first = lazyReflectionMap($reflection);
echo $first['value'], '|', count($first), '|';
LazyReflectionStaticSubject::$value = 12;
echo lazyReflectionValue($reflection), '|';
$second = lazyReflectionMap($reflection);
echo $second['value'];
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "status={} stdout={} stderr={}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "old|assigned|11|1|12|12");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Eval stores owned name strings, which repeated AOT map reads must continue borrowing.
#[test]
fn test_eval_reflector_static_name_lifetimes_bundle() {
    let source = r#"<?php
class EvalStaticNameSubject { public static int $value = 11; }
enum EvalStaticNameEnum {
    case Value;
    public function label(): string { return 'value'; }
}
function readEvalStaticNameMap(ReflectionClass $reflection): mixed {
    return $reflection->getStaticProperties();
}
function printEvalStaticNameMap(ReflectionClass $reflection): void {
    $values = readEvalStaticNameMap($reflection);
    echo count($values), ':', isset($values['value']) ? $values['value'] : '-', '|';
}
function printEvalEnumMembers(ReflectionClass $reflection): void {
    echo (int)$reflection->hasMethod('label'), ':', count($reflection->getMethods()),
        ':', count($reflection->getProperties()), '|';
}
$code = $argc > 0 ? '
    $classOwner = new ReflectionClass("EvalStaticNameSubject");
    $objectOwner = new ReflectionObject(new EvalStaticNameSubject());
    $enumOwner = new ReflectionEnum("EvalStaticNameEnum");
' : '';
eval($code);
printEvalStaticNameMap($classOwner);
$noise = str_repeat('x', strlen(EvalStaticNameSubject::class));
printEvalStaticNameMap($classOwner);
unset($noise);
printEvalStaticNameMap($objectOwner);
$noise = str_repeat('x', strlen(EvalStaticNameSubject::class));
printEvalStaticNameMap($objectOwner);
unset($noise);
printEvalStaticNameMap($enumOwner);
printEvalEnumMembers($enumOwner);
$noise = str_repeat('x', strlen(EvalStaticNameEnum::class));
printEvalStaticNameMap($enumOwner);
unset($noise);
unset($classOwner, $objectOwner, $enumOwner, $code);
echo 'done';
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "status={} stdout={} stderr={}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "1:11|1:11|1:11|1:11|0:-|1:2:1|0:-|done");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Generic class accessors use populated enum slots on builtin and user-derived reflectors.
#[test]
fn test_reflection_enum_inherited_collections_use_enum_metadata() {
    let source = r#"<?php
enum InheritedReflectionEnumSubject: string {
    case First = 'first';
    public const Extra = 9;
    public function label(): string { return $this->value; }
}
class InheritedEnumReflection extends ReflectionEnum {}
function printInheritedEnumCollections(ReflectionClass $reflection): void {
    echo (int)$reflection->hasMethod('label'), ':', count($reflection->getMethods()),
        ':', count($reflection->getProperties()), ':', count($reflection->getConstants()),
        ':', count($reflection->getStaticProperties()), ':', $reflection->getConstant('Extra'), '|';
}
printInheritedEnumCollections(new ReflectionEnum(InheritedReflectionEnumSubject::class));
printInheritedEnumCollections(new InheritedEnumReflection(InheritedReflectionEnumSubject::class));
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "1:4:2:2:0:9|1:4:2:2:0:9|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
