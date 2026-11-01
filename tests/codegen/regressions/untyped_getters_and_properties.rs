//! Purpose:
//! Regression tests for untyped getters and untyped properties fed by several constructor calls.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Until the method pass has walked a body, an untyped `return $this->x;` is typed `int`, so the
//!   FIRST top-level pass refused `count($o->get())`, `array_keys($o->get())`, `in_array()`,
//!   `array_merge()` and `array_map()` even for a property declared `array`; the final pass accepts
//!   them, and those stale first-pass refusals are now dropped.
//! - Each `new C(...)` overwrote an untyped promoted or constructor-assigned property's type with
//!   its own argument's, so the last call checked won: `new A(new A([1]))` raised php-impossible
//!   `TypeError`s ("Cannot assign array to property of type A"). Incompatible arguments now widen
//!   the property to `mixed`.

use crate::support::*;

/// Untyped getters over declared, untyped, promoted and constructor-assigned properties feed
/// array builtins, and one untyped property holds objects, arrays and ints across instances.
#[test]
fn test_untyped_getters_and_widened_untyped_properties() {
    let out = compile_and_run(
        r#"<?php
class Declared { private array $x = ["a" => 1, "b" => 2]; public function get() { return $this->x; } }
class Untyped { private $x = [1, 2]; public function get() { return $this->x; } }
class Promoted { public function __construct(private $x) {} public function get() { return $this->x; } }
class Assigned { private $x; public function __construct($x) { $this->x = $x; } public function get() { return $this->x; } }
$d = new Declared();
echo count($d->get()), implode(",", array_keys($d->get())), in_array(2, $d->get()) ? "y" : "n", "\n";
echo count(array_merge($d->get(), [3])), implode(",", array_map(fn($v) => $v * 2, $d->get())), "\n";
echo count((new Untyped())->get()), count((new Promoted([1, 2, 3]))->get()), count((new Assigned([1]))->get()), "\n";
$o = new Promoted(new stdClass());
$a = new Promoted([4, 5]);
echo get_class($o->get()), count($a->get()), "\n";
$nested = new Promoted(new Promoted([7]));
echo json_encode($nested->get()->get()), "\n";
$i = new Assigned(new Assigned(5));
var_dump($i->get()->get());
"#,
    );
    assert_eq!(out, "2a,by\n32,4\n231\nstdClass2\n[7]\nint(5)\n");
}
