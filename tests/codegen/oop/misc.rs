//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP misc, including inherited constructor specializes base string property type, literal allows sibling objects with common parent, and match without default is fatal.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Uses checked-in example PHP fixtures through include_str! in addition to inline native-output assertions.

use super::*;

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

#[test]
fn test_example_v017_trio_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/v017-trio/main.php"));
    assert_eq!(out, "health:[ok]:missing");
}
