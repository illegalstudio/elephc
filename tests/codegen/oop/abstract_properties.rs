//! Purpose:
//! Integration tests for abstract property declarations.
//! Concrete subclasses must declare every abstract property inherited from their ancestor chain.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Abstract properties have no default and live in an abstract class; concrete redeclarations reuse the parent's slot offset.

use super::*;

#[test]
fn test_abstract_property_concrete_child_declares_default() {
    let out = compile_and_run(
        r#"<?php
abstract class Shape {
    abstract public int $sides;
}

class Square extends Shape {
    public int $sides = 4;
}

$s = new Square();
echo $s->sides;
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_abstract_property_chain_through_abstract_classes() {
    let out = compile_and_run(
        r#"<?php
abstract class A {
    abstract public int $value;
}

abstract class B extends A {
}

class C extends B {
    public int $value = 7;
}

$c = new C();
echo $c->value;
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_abstract_property_typed_invariance_in_concrete_child() {
    let out = compile_and_run(
        r#"<?php
abstract class Box {
    abstract public string $label;
}

class StringBox extends Box {
    public string $label = "hello";
}

$b = new StringBox();
echo $b->label;
"#,
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_abstract_property_set_in_constructor() {
    let out = compile_and_run(
        r#"<?php
abstract class Entity {
    abstract public int $id;
}

class User extends Entity {
    public int $id;

    public function __construct(int $id) {
        $this->id = $id;
    }
}

$u = new User(42);
echo $u->id;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_example_abstract_properties_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/abstract-properties/main.php"));
    assert_eq!(out, "triangle has 3 sides\nsquare has 4 sides\n");
}

#[test]
fn test_abstract_property_concretized_via_promoted_parameter() {
    let out = compile_and_run(
        r#"<?php
abstract class Entity {
    abstract public int $id;
}

class User extends Entity {
    public function __construct(public int $id) {}
}

$u = new User(7);
echo $u->id;
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_abstract_class_with_only_abstract_properties() {
    let out = compile_and_run(
        r#"<?php
abstract class Tagged {
    abstract public int $tag;
    abstract public string $label;
}

class Item extends Tagged {
    public int $tag = 1;
    public string $label = "alpha";
}

$i = new Item();
echo $i->tag;
echo ":";
echo $i->label;
"#,
    );
    assert_eq!(out, "1:alpha");
}

#[test]
fn test_abstract_property_inherited_method_reads_concrete_slot() {
    let out = compile_and_run(
        r#"<?php
abstract class Box {
    abstract public int $value;

    public function show() {
        return $this->value;
    }
}

class IntBox extends Box {
    public int $value = 9;
}

$b = new IntBox();
echo $b->show();
"#,
    );
    assert_eq!(out, "9");
}

#[test]
fn test_abstract_readonly_property_concretized() {
    let out = compile_and_run(
        r#"<?php
abstract class Box {
    abstract public readonly int $value;
}

class IntBox extends Box {
    public readonly int $value;

    public function __construct(int $v) {
        $this->value = $v;
    }
}

$b = new IntBox(42);
echo $b->value;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_abstract_property_nullable_type() {
    let out = compile_and_run(
        r#"<?php
abstract class Source {
    abstract public ?string $name;
}

class Named extends Source {
    public ?string $name = "elephc";
}

$n = new Named();
echo $n->name;
"#,
    );
    assert_eq!(out, "elephc");
}
