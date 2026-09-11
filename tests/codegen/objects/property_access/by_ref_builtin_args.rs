//! Purpose:
//! Regression tests for passing an object property (or a container element reached through
//! one) as the by-reference argument of a mutating array builtin. These calls used to be
//! silent no-ops: the property's array was loaded, separated by copy-on-write inside the
//! runtime helper, mutated, and then discarded because nothing stored it back.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Expected values are real `LC_ALL=C php` 8.4 output for the same fixtures.
//! - The receiver shapes here cover the property-resolution paths the lowering has to walk:
//!   a declared property, an inherited one, a constructor-promoted one, `self::$prop` from a
//!   static method, and a property reached through a typed method parameter.

use super::*;

/// A constructor-promoted, declared-type property is still a writable place for a
/// by-reference builtin argument.
#[test]
fn test_usort_on_promoted_constructor_property() {
    let out = compile_and_run(
        r#"<?php
class B { public function __construct(public array $items) {} }
$b = new B([3,1,2]);
usort($b->items, fn($x, $y) => $x <=> $y);
echo implode(",", $b->items);
"#,
    );
    assert_eq!(out, "1,2,3");
}

/// A declared `array` property may hold associative storage. `usort()` discards those keys,
/// sorts the values, and writes a dense array back after evaluating a nested place only once.
#[test]
fn test_usort_on_declared_assoc_property_reindexes_and_evaluates_place_once() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class B { public function __construct(public array $items) {} }
class Holder { public function __construct(public B $current) {} }
function next_index(int &$calls): int {
    $calls++;
    return 0;
}
$holder = new Holder(new B(["third" => 3, "first" => 1, "second" => 2]));
$owners = [$holder];
$original = $holder->current;
$calls = 0;
usort(
    $owners[next_index($calls)]->current->items,
    function($x, $y) use ($holder) {
        $holder->current = new B(["replacement" => 9]);
        return $x <=> $y;
    },
);
echo implode(",", $original->items), "|", implode(",", $holder->current->items), "|", $calls;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1,2,3|9|1");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected declared-array normalization owners to be released, got: {}",
        out.stderr
    );
}

/// A property inherited from a parent class resolves through the same visible-property
/// lookup, so `sort()` mutates the subclass instance's storage.
#[test]
fn test_sort_on_inherited_property() {
    let out = compile_and_run(
        r#"<?php
class A { public $items = [3,1,2]; }
class B extends A {}
$b = new B();
sort($b->items);
echo implode(",", $b->items);
"#,
    );
    assert_eq!(out, "1,2,3");
}

/// A `self::$prop` receiver inside a static method: the static receiver resolves to the
/// enclosing class before the write-back targets the same static slot.
#[test]
fn test_sort_on_self_static_property_inside_static_method() {
    let out = compile_and_run(
        r#"<?php
class B {
    public static $items = [3,1,2];
    public static function go(): void { sort(self::$items); }
}
B::go();
echo implode(",", B::$items);
"#,
    );
    assert_eq!(out, "1,2,3");
}

/// A property of an object reached through a typed parameter of another class's method, so
/// the receiver resolution starts from a parameter slot rather than a local assignment.
#[test]
fn test_rsort_on_property_of_a_parameter_object() {
    let out = compile_and_run(
        r#"<?php
class B { public $items = [3,1,2]; }
class S { public function run(B $b): void { rsort($b->items); } }
$b = new B();
(new S())->run($b);
echo implode(",", $b->items);
"#,
    );
    assert_eq!(out, "3,2,1");
}

/// Two different array properties of the same object are mutated independently, so the
/// synthetic temporaries do not alias each other.
#[test]
fn test_sorting_two_properties_of_one_object() {
    let out = compile_and_run(
        r#"<?php
class B { public $a = [3,1,2]; public $b = [6,5,4]; }
$o = new B();
sort($o->a);
rsort($o->b);
echo implode(",", $o->a), "|", implode(",", $o->b);
"#,
    );
    assert_eq!(out, "1,2,3|6,5,4");
}
