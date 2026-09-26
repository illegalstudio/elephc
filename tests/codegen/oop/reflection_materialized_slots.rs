//! Purpose:
//! Isolated regressions for Reflection classes materialized by native getter slots.
//!
//! Called from:
//! - `tests/codegen/oop.rs` in the codegen integration test suite.
//!
//! Key details:
//! - Each fixture compiles independently and reaches its companion Reflection class only through
//!   the getter named in `MATERIALIZED_REFLECTION_SLOTS`.
//! - Calling a method on each returned object exercises lowering and vtable generation, not only
//!   the runtime object's tag or string representation.

use crate::support::compile_and_run;

/// `ReflectionObject::getClass()` is the only source of a `ReflectionClass` companion here.
#[test]
fn get_class_materializes_reflection_class_methods() {
    let out = compile_and_run(
        r#"<?php
class MaterializedGetClassTarget {}
function materializedGetClassParameter(MaterializedGetClassTarget $value): void {}
$reflection = new ReflectionParameter("materializedGetClassParameter", "value");
echo $reflection->getClass()->getName();
"#,
    );
    assert_eq!(out, "MaterializedGetClassTarget");
}

/// `getParentClass()` returns a class object the fixture never constructs directly.
#[test]
fn get_parent_class_materializes_reflection_class_methods() {
    let out = compile_and_run(
        r#"<?php
class MaterializedParent {}
class MaterializedChild extends MaterializedParent {}
$reflection = new ReflectionClass(MaterializedChild::class);
echo $reflection->getParentClass()->getName();
"#,
    );
    assert_eq!(out, "MaterializedParent");
}

/// `getInterfaces()` returns class objects used only through the materialized array slot.
#[test]
fn get_interfaces_materializes_reflection_class_methods() {
    let out = compile_and_run(
        r#"<?php
interface MaterializedContract {}
class MaterializedInterfaceTarget implements MaterializedContract {}
$interfaces = (new ReflectionClass(MaterializedInterfaceTarget::class))->getInterfaces();
foreach ($interfaces as $interface) { echo $interface->getName(); }
"#,
    );
    assert_eq!(out, "MaterializedContract");
}

/// `getTraits()` returns class objects used only through the materialized array slot.
#[test]
fn get_traits_materializes_reflection_class_methods() {
    let out = compile_and_run(
        r#"<?php
trait MaterializedTrait {}
class MaterializedTraitTarget { use MaterializedTrait; }
$traits = (new ReflectionClass(MaterializedTraitTarget::class))->getTraits();
foreach ($traits as $trait) { echo $trait->getName(); }
"#,
    );
    assert_eq!(out, "MaterializedTrait");
}

/// `ReflectionParameter::getType()` alone materializes the named-type companion.
#[test]
fn get_type_materializes_reflection_named_type_methods() {
    let out = compile_and_run(
        r#"<?php
function materializedTypedParameter(int $value): void {}
$type = (new ReflectionParameter("materializedTypedParameter", "value"))->getType();
echo get_class($type), ":", $type->getName();
"#,
    );
    assert_eq!(out, "ReflectionNamedType:int");
}

/// `ReflectionFunction::getReturnType()` independently reaches the named-type companion.
#[test]
fn get_return_type_materializes_reflection_named_type_methods() {
    let out = compile_and_run(
        r#"<?php
function materializedTypedReturn(): string { return "ok"; }
$type = (new ReflectionFunction("materializedTypedReturn"))->getReturnType();
echo get_class($type), ":", $type->getName();
"#,
    );
    assert_eq!(out, "ReflectionNamedType:string");
}

/// `ReflectionProperty::getSettableType()` independently materializes the union-type companion.
#[test]
fn get_settable_type_materializes_reflection_union_type_methods() {
    let out = compile_and_run(
        r#"<?php
class MaterializedSettableTypeTarget {
    public private(set) int|string $value;
}
$type = (new ReflectionProperty(MaterializedSettableTypeTarget::class, "value"))->getSettableType();
echo get_class($type), ":";
foreach ($type->getTypes() as $member) { echo $member->getName(), ","; }
"#,
    );
    assert_eq!(out, "ReflectionUnionType:int,string,");
}

/// A getter name assembled at run time reaches the companion through `$holder->$name()`.
///
/// Nothing in the data pool spells the getter: the pieces are `"get"`, `"Declaring"` and a
/// ternary the optimizer cannot fold. The call lowers to a callable descriptor invoke, which is
/// what makes every getter count as named. The result is stringified WITHOUT a method call on it,
/// because a method call would lower the class through the mixed-receiver scan and hide the defect.
#[test]
fn runtime_assembled_getter_name_through_a_dynamic_method_call() {
    let out = compile_and_run(
        r#"<?php
$p = new ReflectionParameter("strlen", "string");
$m = "get" . "Declaring" . ($argc > 5 ? "Class" : "Function");
$f = $p->$m();
echo get_class($f), "|", gettype((string) $f);
"#,
    );
    assert_eq!(out, "ReflectionFunction|string");
}

/// The same runtime-assembled name through `call_user_func([$holder, $name])`.
#[test]
fn runtime_assembled_getter_name_through_call_user_func() {
    let out = compile_and_run(
        r#"<?php
$p = new ReflectionParameter("strlen", "string");
$m = "get" . "Declaring" . ($argc > 5 ? "Class" : "Function");
$f = call_user_func([$p, $m]);
echo get_class($f), "|", gettype((string) $f);
"#,
    );
    assert_eq!(out, "ReflectionFunction|string");
}

/// The same runtime-assembled name through an array callable invoked directly.
#[test]
fn runtime_assembled_getter_name_through_an_array_callable() {
    let out = compile_and_run(
        r#"<?php
$p = new ReflectionParameter("strlen", "string");
$m = "get" . "Declaring" . ($argc > 5 ? "Class" : "Function");
$callable = [$p, $m];
$f = $callable();
echo get_class($f), "|", gettype((string) $f);
"#,
    );
    assert_eq!(out, "ReflectionFunction|string");
}
