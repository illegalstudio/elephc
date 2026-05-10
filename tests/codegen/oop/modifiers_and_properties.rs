//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP class modifiers and properties, including readonly class constructor initialization, final class instantiates and dispatches methods, and final method dispatches normally without override.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Uses checked-in example PHP fixtures through include_str! in addition to inline native-output assertions.

use super::*;

#[test]
fn test_readonly_class_constructor_initialization() {
    let out = compile_and_run(
        r#"<?php
readonly class User {
    public $id;

    public function __construct($id) {
        $this->id = $id;
    }
}

$user = new User(42);
echo $user->id;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_final_class_instantiates_and_dispatches_methods() {
    let out = compile_and_run(
        r#"<?php
final class Receipt {
    public $code = 41;

    public function next() {
        return $this->code + 1;
    }
}

$receipt = new Receipt();
echo $receipt->next();
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_final_method_dispatches_normally_without_override() {
    let out = compile_and_run(
        r#"<?php
class Base {
    final public function label() {
        return "base";
    }
}

class Child extends Base {
    public function suffix() {
        return "child";
    }
}

$child = new Child();
echo $child->label();
echo ":";
echo $child->suffix();
"#,
    );
    assert_eq!(out, "base:child");
}

#[test]
fn test_final_property_reads_normally_without_override() {
    let out = compile_and_run(
        r#"<?php
class Base {
    final public $value = 40;

    public function value() {
        return $this->value + 2;
    }
}

class Child extends Base {
    public function label() {
        return "answer:";
    }
}

$child = new Child();
echo $child->label();
echo $child->value();
"#,
    );
    assert_eq!(out, "answer:42");
}

#[test]
fn test_typed_properties_defaults_constructor_assignment_and_nullable() {
    let out = compile_and_run(
        r#"<?php
class User {
    public int $id;
    public string $name = "Ada";
    public ?string $email = null;

    public function __construct($id) {
        $this->id = $id;
    }

    public function label() {
        return $this->name . ":" . $this->id;
    }
}

$user = new User(42);
echo $user->label();
echo ":";
echo is_null($user->email);
$user->email = "ada@example.test";
echo ":";
echo $user->email;
"#,
    );
    assert_eq!(out, "Ada:42:1:ada@example.test");
}

#[test]
fn test_example_final_classes_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/final-classes/main.php"));
    assert_eq!(out, "invoice:42\n");
}

#[test]
fn test_example_typed_properties_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/typed-properties/main.php"));
    assert_eq!(out, "Ada:42\nmissing email\n");
}
