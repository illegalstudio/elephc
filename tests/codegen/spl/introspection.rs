//! Purpose:
//! End-to-end tests for SPL and class-table introspection helpers.
//! Covers metadata arrays emitted for interfaces, parent classes, and direct trait uses.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the SPL test module.
//!
//! Key details:
//! - The helpers are AOT snapshots and return associative `name => name` arrays.

use crate::support::*;

/// Verifies that class implements returns assoc interface names.
#[test]
fn test_class_implements_returns_assoc_interface_names() {
    let out = compile_and_run(
        r#"<?php
interface BaseMarker {}
interface ChildMarker extends BaseMarker {}
class ImplMarker implements ChildMarker {}

foreach (class_implements("ImplMarker") as $name => $value) {
    echo $name;
    echo "=";
    echo $value;
    echo ";";
}
"#,
    );
    assert_eq!(out, "ChildMarker=ChildMarker;BaseMarker=BaseMarker;");
}

/// Verifies that class implements accepts object static type.
#[test]
fn test_class_implements_accepts_object_static_type() {
    let out = compile_and_run(
        r#"<?php
class Counter implements Countable {
    public function count(): int { return 3; }
}

$interfaces = class_implements(new Counter());
echo isset($interfaces["Countable"]) ? "yes" : "no";
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies that class implements builtin SPL class includes inherited interfaces.
#[test]
fn test_class_implements_builtin_spl_class_includes_inherited_interfaces() {
    let out = compile_and_run(
        r#"<?php
$interfaces = class_implements("SplDoublyLinkedList");
foreach ($interfaces as $name => $value) {
    echo $name;
    echo "=";
    echo $value;
    echo ";";
}
"#,
    );
    assert_eq!(
        out,
        "Iterator=Iterator;Traversable=Traversable;Countable=Countable;ArrayAccess=ArrayAccess;"
    );
}

/// Verifies that class parents returns immediate parent then ancestors.
#[test]
fn test_class_parents_returns_immediate_parent_then_ancestors() {
    let out = compile_and_run(
        r#"<?php
class Root {}
class Middle extends Root {}
class Leaf extends Middle {}

foreach (class_parents("Leaf") as $name => $value) {
    echo $name;
    echo "=";
    echo $value;
    echo ";";
}
"#,
    );
    assert_eq!(out, "Middle=Middle;Root=Root;");
}

/// Verifies that class uses returns direct class traits only.
#[test]
fn test_class_uses_returns_direct_class_traits_only() {
    let out = compile_and_run(
        r#"<?php
trait SharedTrait {}
trait LocalTrait {
    use SharedTrait;
}
class ParentWithTrait {
    use SharedTrait;
}
class ChildWithTrait extends ParentWithTrait {
    use LocalTrait;
}

foreach (class_uses("ChildWithTrait") as $name => $value) {
    echo $name;
    echo "=";
    echo $value;
    echo ";";
}
"#,
    );
    assert_eq!(out, "LocalTrait=LocalTrait;");
}

/// Verifies that class uses accepts trait name.
#[test]
fn test_class_uses_accepts_trait_name() {
    let out = compile_and_run(
        r#"<?php
trait BaseTrait {}
trait CombinedTrait {
    use BaseTrait;
}

foreach (class_uses("CombinedTrait") as $name => $value) {
    echo $name;
    echo "=";
    echo $value;
    echo ";";
}
"#,
    );
    assert_eq!(out, "BaseTrait=BaseTrait;");
}

/// Verifies that class relation helpers return false for unknown literal names.
#[test]
fn test_class_relation_helpers_return_false_for_unknown_literal_names() {
    let out = compile_and_run(
        r#"<?php
var_dump(class_implements("MissingClass"));
var_dump(class_parents("MissingClass"));
var_dump(class_uses("MissingClass"));
"#,
    );
    assert_eq!(out, "bool(false)\nbool(false)\nbool(false)\n");
}
