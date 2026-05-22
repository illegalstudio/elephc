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
