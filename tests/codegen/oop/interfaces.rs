//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP interfaces, including interface contract can be satisfied by concrete class, abstract base can defer method to concrete child, and class can implement multiple interfaces.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Uses checked-in example PHP fixtures through include_str! in addition to inline native-output assertions.

use super::*;

/// Verifies a concrete class can satisfy an interface contract by implementing all required methods.
/// Fixture: interface `Named` with method `name()`, concrete `User` implementing `Named`.
/// Asserts the method call on the concrete instance returns the expected string.
#[test]
fn test_interface_contract_can_be_satisfied_by_concrete_class() {
    let out = compile_and_run(
        r#"<?php
interface Named {
    public function name();
}

class User implements Named {
    public function name() {
        return "Ada";
    }
}

$user = new User();
echo $user->name();
"#,
    );
    assert_eq!(out, "Ada");
}

/// Verifies an abstract class can defer interface method implementation to a concrete child class.
/// Fixture: abstract `BaseGreeter` with abstract method `label()` and concrete `PersonGreeter`.
/// Asserts calling `greet()` on the concrete child triggers `label()` via `$this->label()`.
#[test]
fn test_abstract_base_can_defer_method_to_concrete_child() {
    let out = compile_and_run(
        r#"<?php
abstract class BaseGreeter {
    abstract public function label();

    public function greet() {
        return "hi " . $this->label();
    }
}

class PersonGreeter extends BaseGreeter {
    public function label() {
        return "world";
    }
}

$g = new PersonGreeter();
echo $g->greet();
"#,
    );
    assert_eq!(out, "hi world");
}

/// Verifies a class can implement multiple interfaces simultaneously.
/// Fixture: `Named` and `Tagged` interfaces, `Item` implementing both.
/// Asserts chained method calls resolve to the correct interface method on the same instance.
#[test]
fn test_class_can_implement_multiple_interfaces() {
    let out = compile_and_run(
        r#"<?php
interface Named {
    public function name();
}

interface Tagged {
    public function tag();
}

class Item implements Named, Tagged {
    public function name() {
        return "box";
    }

    public function tag() {
        return "BX";
    }
}

$item = new Item();
echo $item->name() . ":" . $item->tag();
"#,
    );
    assert_eq!(out, "box:BX");
}

/// Verifies transitive interface extension is enforced: a class must satisfy the full chain.
/// Fixture: `Labeled extends Named`, `Product implements Labeled`. Uses `strtoupper($this->name())`.
/// Asserts the method call correctly resolves through the transitive interface hierarchy.
#[test]
fn test_transitive_interface_extends_is_enforced() {
    let out = compile_and_run(
        r#"<?php
interface Named {
    public function name();
}

interface Labeled extends Named {
    public function label();
}

class Product implements Labeled {
    public function name() {
        return "widget";
    }

    public function label() {
        return strtoupper($this->name());
    }
}

$product = new Product();
echo $product->label();
"#,
    );
    assert_eq!(out, "WIDGET");
}

/// Verifies the checked-in example at `examples/interfaces/main.php` compiles and runs end-to-end.
/// Loads the PHP fixture via `include_str!`, asserts stdout matches expected multi-line output.
#[test]
fn test_example_interfaces_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/interfaces/main.php"));
    // `isset(...) . "\n"`: a bool false stringifies to "" (not "0") in PHP, so the
    // post-unset isset line is empty.
    assert_eq!(out, "WIDGET\nA-42\n1\n\n");
}

/// Verifies an interface with a read-only property (`get;`) can be satisfied by a concrete property.
/// Fixture: interface `HasId` with `public int $id { get; }`, concrete `User` with int field.
/// Asserts reading the property on the concrete instance returns the expected value.
#[test]
fn test_interface_get_property_contract_is_satisfied_by_concrete_property() {
    let out = compile_and_run(
        r#"<?php
interface HasId {
    public int $id { get; }
}

class User implements HasId {
    public int $id = 42;
}

$user = new User();
echo $user->id;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies interface property setters allow contravariant type (subclass) in implementing class.
/// Fixture: `Dog extends Animal`, interface `DogSink` with `public Dog $pet { set; }`,
/// implementing `Kennel` declares `public Animal $pet`. Sets a `Dog` instance and checks `instanceof Animal`.
/// Asserts contravariant property types are accepted per PHP semantics.
#[test]
fn test_interface_set_property_contract_allows_contravariant_type() {
    let out = compile_and_run(
        r#"<?php
class Animal {}
class Dog extends Animal {}

interface DogSink {
    public Dog $pet { set; }
}

class Kennel implements DogSink {
    public Animal $pet;
}

$kennel = new Kennel();
$kennel->pet = new Dog();
echo $kennel->pet instanceof Animal;
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies an abstract class can defer interface property implementation to a concrete child.
/// Fixture: interface `HasName` with `string $name { get; set; }`, abstract `NamedBase implements HasName`,
/// concrete `Product extends NamedBase` with a default field initializer.
/// Asserts reading the property on the concrete child resolves via the abstract's interface contract.
#[test]
fn test_abstract_class_can_defer_interface_property_to_child() {
    let out = compile_and_run(
        r#"<?php
interface HasName {
    public string $name { get; set; }
}

abstract class NamedBase implements HasName {
}

class Product extends NamedBase {
    public string $name = "widget";
}

$product = new Product();
echo $product->name;
"#,
    );
    assert_eq!(out, "widget");
}
