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
