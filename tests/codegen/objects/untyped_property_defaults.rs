//! Purpose:
//! Integration tests for untyped property defaults: PHP initializes untyped properties
//! without a default (and with an explicit `= null` default) to null, even when later
//! assignments give the slot a concrete scalar/array type.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.
//! - Untyped null-defaulted properties must ride the same nullable storage as typed `?T` properties.

use super::*;

/// Verifies that an untyped instance property with no default reads as null before any assignment.
#[test]
fn test_untyped_property_without_default_defaults_to_null() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $never;
}
$c = new Car();
var_dump($c->never);
"#,
    );
    assert_eq!(out, "NULL\n");
}

/// Verifies that an untyped no-default property later assigned an int still defaults to null.
#[test]
fn test_untyped_property_without_default_assigned_int_later() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $pippo;
}
$c = new Car();
var_dump($c->pippo);
$c->pippo = 5;
var_dump($c->pippo);
"#,
    );
    assert_eq!(out, "NULL\nint(5)\n");
}

/// Verifies that an untyped no-default property later assigned a string still defaults to null.
#[test]
fn test_untyped_property_without_default_assigned_string_later() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $s;
}
$c = new Car();
var_dump($c->s);
$c->s = "hi";
var_dump($c->s);
"#,
    );
    assert_eq!(out, "NULL\nstring(2) \"hi\"\n");
}

/// Verifies that an untyped `= null` property later assigned an int compiles and keeps its null default.
#[test]
fn test_untyped_property_explicit_null_default_assigned_int_later() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $pippo = null;
}
$c = new Car();
var_dump($c->pippo);
$c->pippo = 5;
var_dump($c->pippo);
"#,
    );
    assert_eq!(out, "NULL\nint(5)\n");
}

/// Verifies that an untyped `= null` property later assigned an array var_dumps NULL before the write.
#[test]
fn test_untyped_property_explicit_null_default_assigned_array_later() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $arr = null;
}
$c = new Car();
var_dump($c->arr);
$c->arr = [1, 2];
echo $c->arr[1], "\n";
"#,
    );
    assert_eq!(out, "NULL\n2\n");
}

/// Verifies bool and float assignments to untyped `= null` properties keep the null default.
#[test]
fn test_untyped_property_explicit_null_default_assigned_bool_and_float() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $b = null;
    public $f = null;
}
$c = new Car();
var_dump($c->b);
var_dump($c->f);
$c->b = true;
$c->f = 1.5;
var_dump($c->b);
var_dump($c->f);
"#,
    );
    assert_eq!(out, "NULL\nNULL\nbool(true)\nfloat(1.5)\n");
}

/// Verifies is_null()/`=== null` observe the null default of an untyped property before and after a write.
#[test]
fn test_untyped_property_null_default_strict_null_comparison() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $x = null;
}
$c = new Car();
echo is_null($c->x) ? "y" : "n";
echo ($c->x === null) ? "y" : "n";
$c->x = 5;
echo is_null($c->x) ? "y" : "n";
echo ($c->x === null) ? "y" : "n";
echo "\n";
"#,
    );
    assert_eq!(out, "yynn\n");
}

/// Verifies that an untyped static property with no default reads as null before any assignment.
#[test]
fn test_untyped_static_property_without_default_defaults_to_null() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public static $sp;
}
var_dump(Car::$sp);
Car::$sp = 5;
var_dump(Car::$sp);
"#,
    );
    assert_eq!(out, "NULL\nint(5)\n");
}

/// Verifies that an untyped `= null` static property later assigned a string keeps its null default.
#[test]
fn test_untyped_static_property_explicit_null_default_assigned_string_later() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public static $spn = null;
}
var_dump(Car::$spn);
Car::$spn = "hi";
var_dump(Car::$spn);
"#,
    );
    assert_eq!(out, "NULL\nstring(2) \"hi\"\n");
}

/// Verifies heterogeneous assignments to an untyped `= null` property follow last-write-wins like PHP.
#[test]
fn test_untyped_property_heterogeneous_assignments_last_value_wins() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $h = null;
}
$c = new Car();
var_dump($c->h);
$c->h = 1;
var_dump($c->h);
$c->h = "s";
var_dump($c->h);
"#,
    );
    assert_eq!(out, "NULL\nint(1)\nstring(1) \"s\"\n");
}

/// Verifies the already-working untyped object-holding property keeps its null default (regression guard).
#[test]
fn test_untyped_property_object_null_default_still_works() {
    let out = compile_and_run(
        r#"<?php
class Dep {
    public function id(): int {
        return 7;
    }
}
class Holder {
    public $obj = null;
}
$h = new Holder();
var_dump($h->obj);
$h->obj = new Dep();
echo $h->obj->id(), "\n";
"#,
    );
    assert_eq!(out, "NULL\n7\n");
}

/// Verifies untyped properties with a concrete non-null default are unaffected (regression guard).
#[test]
fn test_untyped_property_nonnull_default_unchanged() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $n = 1;
    public $s = "a";
}
$c = new Car();
var_dump($c->n);
var_dump($c->s);
"#,
    );
    assert_eq!(out, "int(1)\nstring(1) \"a\"\n");
}
