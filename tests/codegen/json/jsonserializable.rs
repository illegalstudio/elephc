//! Purpose:
//! Provides JsonSerializable builtin interface tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - The injected interface must type-check declarations and instanceof checks.

use super::*;

// JsonSerializable is exposed as a builtin interface so user-defined classes
// can declare `implements JsonSerializable` and the type checker accepts the
// abstract `jsonSerialize(): mixed` contract. The encoder dispatches through
// jsonSerialize() when classes implement the interface.

/// Verifies a class can declare `implements JsonSerializable` and the type checker accepts it.
/// Tests that the injected interface provides the abstract `jsonSerialize(): mixed` contract.
#[test]
fn test_jsonserializable_class_declaration_compiles() {
    let out = compile_and_run(
        r#"<?php
class Item implements JsonSerializable {
    public string $name;
    public function __construct(string $name) { $this->name = $name; }
    public function jsonSerialize(): mixed { return $this->name; }
}
$it = new Item("widget");
echo $it->name;
"#,
    );
    assert_eq!(out, "widget");
}

/// Verifies a class implementing JsonSerializable passes `instanceof JsonSerializable`.
#[test]
fn test_jsonserializable_instanceof_check() {
    let out = compile_and_run(
        r#"<?php
class Tag implements JsonSerializable {
    public function jsonSerialize(): mixed { return "x"; }
}
$t = new Tag();
echo ($t instanceof JsonSerializable ? "yes" : "no");
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies a class without `implements JsonSerializable` is not an instanceof JsonSerializable.
#[test]
fn test_class_without_jsonserializable_is_not_instance() {
    let out = compile_and_run(
        r#"<?php
class Plain { public int $n = 0; }
$p = new Plain();
echo ($p instanceof JsonSerializable ? "yes" : "no");
"#,
    );
    assert_eq!(out, "no");
}

/// Verifies an implementing class's jsonSerialize() returning mixed is accepted by the type checker.
#[test]
fn test_jsonserialize_method_returns_mixed_type() {
    // The interface's abstract method declares `mixed`; an implementing
    // class can return any concrete type and the checker is satisfied.
    let out = compile_and_run(
        r#"<?php
class Counter implements JsonSerializable {
    public int $value;
    public function __construct(int $v) { $this->value = $v; }
    public function jsonSerialize(): mixed { return $this->value; }
}
$c = new Counter(7);
echo gettype($c) . ":" . $c->value;
"#,
    );
    assert_eq!(out, "object:7");
}
