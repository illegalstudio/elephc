//! Purpose:
//! Integration or regression tests for PHP object cloning codegen.
//! Covers shallow object copies, declared property slots, stdClass dynamic properties, and `__clone` hooks.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries and compare stdout against PHP clone semantics.

use super::*;

/// Verifies cloning declared scalar/string properties creates an independent object slot copy.
#[test]
fn test_clone_copies_declared_properties_independently() {
    let out = compile_and_run(
        r#"<?php
class Item {
    public int $n = 1;
    public string $label = "one";
}
$a = new Item();
$b = clone $a;
$b->n = 2;
$b->label = "two";
echo $a->n . ":" . $a->label . "|" . $b->n . ":" . $b->label;
"#,
    );
    assert_eq!(out, "1:one|2:two");
}

/// Verifies `__clone()` is invoked after the shallow copy and mutates the clone, not the source.
#[test]
fn test_clone_invokes_magic_clone_on_the_copy() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public int $n = 1;
    public function __clone(): void {
        echo "hook;";
        $this->n = $this->n + 10;
    }
}
$a = new Counter();
$b = clone $a;
echo $a->n . "|" . $b->n;
"#,
    );
    assert_eq!(out, "hook;1|11");
}

/// Verifies `__clone()` can replace a string property without corrupting the source object.
#[test]
fn test_clone_persists_string_property_before_magic_clone_mutation() {
    let out = compile_and_run(
        r#"<?php
class LabelBox {
    public string $label = "A";
    public function __clone(): void {
        $this->label = $this->label . ":copy";
    }
}
$a = new LabelBox();
$b = clone $a;
echo $a->label . "|" . $b->label;
"#,
    );
    assert_eq!(out, "A|A:copy");
}

/// Verifies a cloned string property remains valid after the source owner is released.
#[test]
fn test_clone_string_property_survives_source_release() {
    let out = compile_and_run(
        r#"<?php
class LabelOwner {
    public string $label = "";
}
$a = new LabelOwner();
$a->label = date_default_timezone_get();
$b = clone $a;
unset($a);
echo $b->label;
"#,
    );
    assert_eq!(out, "UTC");
}

/// Verifies object-valued properties are shallow-copied, so nested object mutations remain shared.
#[test]
fn test_clone_keeps_nested_objects_shared() {
    let out = compile_and_run(
        r#"<?php
class Child {
    public int $x = 1;
}
class Boxed {
    public Child $child;
    public function __construct() {
        $this->child = new Child();
    }
}
$a = new Boxed();
$b = clone $a;
$b->child->x = 7;
echo $a->child->x . "|" . $b->child->x;
"#,
    );
    assert_eq!(out, "7|7");
}

/// Verifies stdClass dynamic properties are copied into a separate hash table during cloning.
#[test]
fn test_clone_copies_stdclass_dynamic_properties_independently() {
    let out = compile_and_run(
        r#"<?php
$a = new stdClass();
$a->name = "source";
$b = clone $a;
$b->name = "copy";
$b->extra = "new";
echo $a->name . "|" . $b->name . "|" . (isset($a->extra) ? "Y" : "N");
"#,
    );
    assert_eq!(out, "source|copy|N");
}

/// Verifies a discarded standalone clone expression still runs the clone operation safely.
#[test]
fn test_standalone_clone_expression() {
    let out = compile_and_run(
        r#"<?php
class StandaloneClone {}
$object = new StandaloneClone();
clone $object;
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies clone recursively resolves case-insensitive class names in its operand.
#[test]
fn test_clone_resolves_case_insensitive_new_object_name() {
    let out = compile_and_run(
        r#"<?php
class CloneCaseProbe {}
$copy = clone new Clonecaseprobe();
echo get_class($copy);
"#,
    );
    assert_eq!(out, "CloneCaseProbe");
}

/// Verifies a clone operand widened to boxed `Mixed` is checked at runtime, cloned through its
/// concrete object class, and remains independent from the original DateTime payload.
#[test]
fn test_clone_runtime_mixed_datetime_object() {
    let out = compile_and_run(
        r#"<?php
function obscure_clone_value(mixed $value): mixed {
    return $value;
}
$source = new DateTime("2024-01-01T00:00:00Z");
$copy = clone obscure_clone_value($source);
$copy->modify("+1 day");
echo $source->format("Y-m-d"), "|", $copy->format("Y-m-d"), "|";
try {
    clone obscure_clone_value(7);
} catch (TypeError $error) {
    echo get_class($error);
}
"#,
    );
    assert_eq!(out, "2024-01-01|2024-01-02|TypeError");
}
