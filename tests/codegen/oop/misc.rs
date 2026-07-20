//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP misc, including inherited constructor specializes base string property type, literal allows sibling objects with common parent, and match without default is fatal.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Uses checked-in example PHP fixtures through include_str! in addition to inline native-output assertions.

use super::*;

/// Verifies PHP's generic `object` parameter type accepts concrete objects and
/// preserves object-shaped ABI lowering.
#[test]
fn test_generic_object_parameter_type_accepts_concrete_object() {
    let out = compile_and_run(
        r#"<?php
class GenericObjectParam {}
function accepts_object(object $value): string {
    return is_object($value) ? "object" : "bad";
}
echo accepts_object(new GenericObjectParam());
"#,
    );
    assert_eq!(out, "object");
}

/// Verifies lowercase and mixed-case `object` hints remain generic object types inside a
/// namespace instead of being rewritten to namespace-local class names.
#[test]
fn test_namespaced_generic_object_parameter_type_is_not_prefixed() {
    let out = compile_and_run(
        r#"<?php
namespace App;

class NamespacedObjectValue {}

function accepts_lower_object(object $value): string {
    return is_object($value) ? "lower" : "bad";
}

function accepts_mixed_case_object(Object $value): string {
    return is_object($value) ? "upper" : "bad";
}

$value = new NamespacedObjectValue();
echo accepts_lower_object($value) . "|" . accepts_mixed_case_object($value);
"#,
    );
    assert_eq!(out, "lower|upper");
}

/// Tests that a Child class inheriting Base's constructor properly specializes the
/// base class's string property type, so `new Child("Ada")` works without explicit
/// constructor in the child.
///
/// Fixture: Base with `$name` property and typed constructor; Child extends Base with
/// no constructor. Verifies greet() returns the passed name.
#[test]
fn test_inherited_constructor_specializes_base_string_property_type() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public $name;

    public function __construct($name) {
        $this->name = $name;
    }

    public function greet() {
        return $this->name;
    }
}

class Child extends Base {}

$child = new Child("Ada");
echo $child->greet();
"#,
    );
    assert_eq!(out, "Ada");
}

/// Tests that array literals can contain sibling objects that share a common parent
/// class, using a foreach loop to iterate and call a parent method.
///
/// Fixture: Animal base with `$name`; Dog and Cat subclasses; array literal with
/// `new Dog("Rex")` and `new Cat("Mia")`. Verifies both labels are printed.
#[test]
fn test_array_literal_allows_sibling_objects_with_common_parent() {
    let out = compile_and_run(
        r#"<?php
class Animal {
    public $name;

    public function __construct($name) {
        $this->name = $name;
    }

    public function label() {
        return $this->name;
    }
}

class Dog extends Animal {}
class Cat extends Animal {}

$animals = [new Dog("Rex"), new Cat("Mia")];
foreach ($animals as $animal) {
    echo $animal->label() . " ";
}
"#,
    );
    assert_eq!(out, "Rex Mia ");
}

/// Tests that a match expression without a default case produces a fatal error
/// ("unhandled match case") when the matched value has no corresponding arm.
///
/// Fixture: `$value = 3` matched against arms 1 and 2 only. Verifies fatal error
/// message is produced.
#[test]
fn test_match_without_default_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
$value = 3;
echo match($value) {
    1 => "one",
    2 => "two",
};
"#,
    );
    assert!(err.contains("unhandled match case"), "{err}");
}

/// Verifies the v017-trio example PHP fixture compiles and runs, asserting the
/// expected output "health:[ok]:missing".
///
/// Fixture: `examples/v017-trio/main.php` via `include_str!`. Regression guard for
/// trio example staying working.
#[test]
fn test_example_v017_trio_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/v017-trio/main.php"));
    assert_eq!(out, "health:[ok]:missing");
}

/// EC-10: `enum` is only a soft keyword — `class Enum {}` / `interface Enum` / `new Enum`
/// are legal PHP (vendor precedent: marc-mabe/php-enum). Byte-parity vs PHP 8.5.
#[test]
fn test_class_named_enum_declares() {
    let out = compile_and_run(
        "<?php class Enum { public function tag(): string { return 'e'; } } echo (new Enum())->tag();",
    );
    assert_eq!(out, "e");
}

/// Soft-keyword `enum` is also legal as an interface name.
#[test]
fn test_interface_named_enum_declares() {
    let out = compile_and_run(
        "<?php interface Enum { public function tag(): string; } class C implements Enum { public function tag(): string { return 'i'; } } echo (new C())->tag();",
    );
    assert_eq!(out, "i");
}
