//! Purpose:
//! Regression tests for the review findings of the object clone, dynamic `new`, hooked property
//! and polymorphic / `Mixed` property read and write lowering.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected output is what `php -n` printed for the same program.

use crate::support::*;

/// An untyped backed hooked property without a default is born null, not uninitialized.
#[test]
fn test_untyped_hooked_property_without_default_starts_null() {
    let out = compile_and_run(
        r#"<?php
class A { public $x { set { $this->x = strtoupper($value); } } }
$a = new A;
var_dump($a);
var_dump($a->x);
$a->x = "q";
var_dump($a->x);
"#,
    );
    assert_eq!(
        out,
        "object(A)#1 (1) {\n  [\"x\"]=>\n  NULL\n}\nNULL\nstring(1) \"Q\"\n"
    );
}

/// A spread `new $class(...$args)` keeps a class whose first constructor parameter is `?int`.
#[test]
fn test_dynamic_new_spread_keeps_nullable_int_constructor_class() {
    let out = compile_and_run(
        r#"<?php
class P { function __construct(public ?int $n = null) {} }
$c = 'P';
$a = [5];
$o = new $c(...$a);
var_dump($o->n);
"#,
    );
    assert_eq!(out, "int(5)\n");
}

/// A read through an interface-typed receiver honours visibility and `__get` of the implementers.
#[test]
fn test_interface_typed_property_read_respects_visibility_and_magic_get() {
    let out = compile_and_run(
        r#"<?php
interface HasId {}
class User implements HasId { private int $id = 7; }
class Lazy implements HasId { function __get($n) { return 1; } }
class Open implements HasId { public int $id = 3; }
function show(HasId $h) { return $h->id; }
try { echo show(new User), "\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
var_dump(show(new Lazy));
var_dump(show(new Open));
"#,
    );
    assert_eq!(
        out,
        "Cannot access private property User::$id\nint(1)\nint(3)\n"
    );
}

/// A `Mixed` read of a dynamic property bound by reference yields the referenced value.
#[test]
fn test_mixed_read_follows_dynamic_property_reference_marker() {
    let out = compile_and_run(
        r#"<?php
#[AllowDynamicProperties] class D {}
$o = new D; $o->v = 1; $r = &$o->v; $r = 2;
foreach ([$o, 5] as $x) { if (is_object($x)) { echo $x->v, "\n"; } }
"#,
    );
    assert_eq!(out, "2\n");
}

/// A `Mixed` read finds a dynamic property the object holds before it calls `__get`.
#[test]
fn test_mixed_read_probes_dynamic_hash_before_magic_get() {
    let out = compile_and_run(
        r#"<?php
#[AllowDynamicProperties] class Lazy { function __get($n) { echo "load\n"; return $this->$n = new stdClass; } }
$l = new Lazy;
foreach ([$l, $l, 5] as $x) { if (is_object($x)) { $x->svc; } }
"#,
    );
    assert_eq!(out, "load\n");
}

/// A write through a bare `object` receiver refuses a private slot and a second readonly write.
#[test]
fn test_bare_object_property_write_respects_scope_and_readonly() {
    let out = compile_and_run(
        r#"<?php
class A { private $secret = 1; public readonly int $r; function __construct() { $this->r = 5; } function get() { return $this->secret; } }
function poke(object $o) { try { $o->secret = 2; } catch (Error $e) { echo $e->getMessage(), "\n"; } }
function repoke(object $o) { try { $o->r = 6; } catch (Error $e) { echo $e->getMessage(), "\n"; } }
$a = new A;
poke($a);
repoke($a);
echo $a->get(), $a->r, "\n";
"#,
    );
    assert_eq!(
        out,
        "Cannot access private property A::$secret\nCannot modify readonly property A::$r\n15\n"
    );
}

/// A raw empty array stored through a class ladder into a keyed-array slot stays a usable hash.
#[test]
fn test_empty_array_stored_through_object_receiver_into_keyed_slot() {
    let out = compile_and_run(
        r#"<?php
class A { public array $m = ['k' => 1]; }
function reset_it(object $o) { $o->m = []; }
$a = new A;
reset_it($a);
$a->m['x'] = 2;
echo count($a->m), " ", $a->m['x'], "\n";
"#,
    );
    assert_eq!(out, "1 2\n");
}
