//! Purpose:
//! Regression tests for mutating array builtins whose by-reference argument is a *place*
//! other than a plain local: an object property, a static property, or a container element.
//! Every one of these used to compile to a silent no-op — the runtime separated a
//! copy-on-write copy that nothing stored back — so the assertions here are the guard against
//! that class of silent wrong answer returning.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected value is real `LC_ALL=C php` (8.4) output for the same fixture.
//! - The copy-on-write cases are the important ones: PHP separates the array before mutating
//!   it, so an alias taken before the call must still observe the original element order.
//! - `array_unshift` fixtures double as coverage for the payload growth the runtime helper
//!   needs; without it a full array wrote one slot past its allocation.

use crate::support::*;

/// The original silent miscompilation: `usort()` on an instance property left the property
/// untouched with no diagnostic. Verifies the sorted order now reaches the property.
#[test]
fn test_usort_on_instance_property_sorts_in_place() {
    let out = compile_and_run(
        r#"<?php
class B { public $items = [3,1,2]; }
$b = new B();
usort($b->items, fn($x, $y) => $x <=> $y);
echo implode(",", $b->items);
"#,
    );
    assert_eq!(out, "1,2,3");
}

/// The same regression for a string-element array, so the fix is not specific to the
/// integer sort helper.
#[test]
fn test_usort_on_instance_property_sorts_strings_in_place() {
    let out = compile_and_run(
        r#"<?php
class B { public $items = ["pear","apple","fig"]; }
$b = new B();
usort($b->items, fn($x, $y) => strcmp($x, $y));
echo implode(",", $b->items);
"#,
    );
    assert_eq!(out, "apple,fig,pear");
}

/// `sort()` and `rsort()` both mutate an instance property, confirming the rewrite is driven
/// by the by-reference parameter rather than by one builtin's lowering.
#[test]
fn test_sort_and_rsort_on_instance_property() {
    let out = compile_and_run(
        r#"<?php
class B { public $items = [3,1,2]; }
$b = new B();
sort($b->items);
echo implode(",", $b->items), "|";
rsort($b->items);
echo implode(",", $b->items);
"#,
    );
    assert_eq!(out, "1,2,3|3,2,1");
}

/// The whole structural-mutator family on one instance property: push, pop, shift, unshift,
/// and splice each have to observe and update the same property storage in sequence.
#[test]
fn test_structural_mutators_on_instance_property() {
    let out = compile_and_run(
        r#"<?php
class B { public $items = [3,1,2]; }
$b = new B();
array_push($b->items, 9);
echo implode(",", $b->items), "|";
echo array_pop($b->items), "|";
echo array_shift($b->items), "|";
array_unshift($b->items, 7);
echo implode(",", $b->items), "|";
array_splice($b->items, 1, 1);
echo implode(",", $b->items);
"#,
    );
    assert_eq!(out, "3,1,2,9|9|3|7,1,2|7,2");
}

/// A static property receiver: `sort()` mutated it only while it was unaliased, and
/// `array_push()`/`usort()` on it were silent no-ops once a copy existed.
#[test]
fn test_sort_family_on_static_property() {
    let out = compile_and_run(
        r#"<?php
class B { public static $items = [3,1,2]; }
sort(B::$items);
echo implode(",", B::$items), "|";
array_push(B::$items, 0);
usort(B::$items, fn($x, $y) => $y <=> $x);
echo implode(",", B::$items);
"#,
    );
    assert_eq!(out, "1,2,3|3,2,1,0");
}

/// A hash element holding a nested array: `sort($m["k"])` used to sort a discarded copy.
#[test]
fn test_sort_on_string_keyed_array_element() {
    let out = compile_and_run(
        r#"<?php
$m = ["k" => [3,1,2], "j" => [5,4]];
sort($m["k"]);
rsort($m["j"]);
echo implode(",", $m["k"]), "|", implode(",", $m["j"]);
"#,
    );
    assert_eq!(out, "1,2,3|5,4");
}

/// An indexed element holding a nested array. This shape previously failed EIR validation
/// because the element-address by-reference path only models scalar element cells.
#[test]
fn test_sort_on_indexed_array_element() {
    let out = compile_and_run(
        r#"<?php
$a = [[3,1,2],[9,8]];
usort($a[0], fn($x, $y) => $x <=> $y);
array_push($a[1], 7);
echo implode(",", $a[0]), "|", implode(",", $a[1]);
"#,
    );
    assert_eq!(out, "1,2,3|9,8,7");
}

/// Copy-on-write on a property receiver: PHP separates the array before sorting, so the
/// alias taken before the call keeps the original element order.
#[test]
fn test_sort_on_instance_property_respects_copy_on_write() {
    let out = compile_and_run(
        r#"<?php
class B { public $items = [3,1,2]; }
$b = new B();
$copy = $b->items;
usort($b->items, fn($x, $y) => $x <=> $y);
echo implode(",", $b->items), "|", implode(",", $copy);
"#,
    );
    assert_eq!(out, "1,2,3|3,1,2");
}

/// Copy-on-write on a static-property receiver. A static-property load carries no reference
/// of its own, so an implementation that moved the borrowed pointer into a temporary would
/// free the array the alias still holds when the write-back released the previous occupant.
#[test]
fn test_sort_on_static_property_respects_copy_on_write() {
    let out = compile_and_run(
        r#"<?php
class B { public static $items = [3,1,2]; }
$copy = B::$items;
sort(B::$items);
echo implode(",", B::$items), "|", implode(",", $copy);
"#,
    );
    assert_eq!(out, "1,2,3|3,1,2");
}

/// Copy-on-write on a container-element receiver.
#[test]
fn test_sort_on_array_element_respects_copy_on_write() {
    let out = compile_and_run(
        r#"<?php
$m = ["k" => [3,1,2]];
$copy = $m["k"];
sort($m["k"]);
echo implode(",", $m["k"]), "|", implode(",", $copy);
"#,
    );
    assert_eq!(out, "1,2,3|3,1,2");
}

/// A `$this->prop` receiver inside a method body.
#[test]
fn test_usort_on_this_property_inside_method() {
    let out = compile_and_run(
        r#"<?php
class B {
    public $items = [3,1,2];
    public function sortItems(): void { usort($this->items, fn($x, $y) => $x <=> $y); }
}
$b = new B();
$b->sortItems();
echo implode(",", $b->items);
"#,
    );
    assert_eq!(out, "1,2,3");
}

/// A two-link property chain (`$outer->inner->items`), so the receiver resolution walks more
/// than one property hop before it finds the array storage.
#[test]
fn test_sort_on_nested_property_chain() {
    let out = compile_and_run(
        r#"<?php
class Inner { public $items = [3,1,2]; }
class Outer { public $inner; public function __construct() { $this->inner = new Inner(); } }
$o = new Outer();
sort($o->inner->items);
echo implode(",", $o->inner->items);
"#,
    );
    assert_eq!(out, "1,2,3");
}

/// The element index of a by-reference place is evaluated exactly once, even though the place
/// is read before the call and written after it.
#[test]
fn test_by_ref_element_index_is_evaluated_once() {
    let out = compile_and_run(
        r#"<?php
$m = [[3,1,2],[5,4]];
$calls = 0;
function pick(&$calls) { $calls = $calls + 1; return 0; }
sort($m[pick($calls)]);
echo implode(",", $m[0]), "|", $calls;
"#,
    );
    assert_eq!(out, "1,2,3|1");
}

/// A by-reference place passed as a *named* argument binds to the same parameter and takes the
/// same rewrite, including when the named arguments are written out of parameter order.
#[test]
fn test_named_by_ref_argument_on_property_mutates_property() {
    let out = compile_and_run(
        r#"<?php
class B { public $items = [3,1,2]; }
$b = new B();
$copy = $b->items;
sort(array: $b->items);
echo implode(",", $b->items), "|", implode(",", $copy), "|";
$c = new B();
usort(callback: fn($x, $y) => $y <=> $x, array: $c->items);
echo implode(",", $c->items);
"#,
    );
    assert_eq!(out, "1,2,3|3,1,2|3,2,1");
}

/// `shuffle()` on a property permutes the property's own storage. The permutation is random,
/// so the assertion checks the multiset and length rather than an order.
#[test]
fn test_shuffle_on_instance_property_permutes_property_storage() {
    let out = compile_and_run(
        r#"<?php
class B { public $items = [5,3,1,4,2]; }
$b = new B();
shuffle($b->items);
$c = $b->items;
sort($c);
echo implode(",", $c), "|", count($b->items);
"#,
    );
    assert_eq!(out, "1,2,3,4,5|5");
}

/// `array_unshift()` on a full array must grow the payload before shifting. Without the
/// growth it wrote one element past the allocation and left `length > capacity`, so the next
/// copy-on-write split produced an over-long copy whose tail read adjacent heap header words.
#[test]
fn test_array_unshift_grows_before_prepending() {
    let out = compile_and_run(
        r#"<?php
$p = [1,2,3,0];
array_unshift($p, 7);
$q = $p;
rsort($q);
echo implode(",", $q), "|", implode(",", $p);
"#,
    );
    assert_eq!(out, "7,3,2,1,0|7,1,2,3,0");
}

/// Verifies the by-reference REFUSAL guard still accepts every place that has storage.
///
/// A builtin reaches its by-reference argument through the storage itself, so every form
/// below compiles and runs — and all of them would have stopped compiling had the guard that
/// refuses `array_push([1], 2)` reused the USER-function predicate. That one accepts only
/// variables and array elements, because a user call writes its result back to a local SLOT
/// and a property has none. The property and static-property cases are what pin the two
/// predicates apart; the refusals themselves live in the error tests.
#[test]
fn test_by_ref_builtin_parameter_accepts_every_place_with_storage() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public array $items = [3, 1];
    public static array $shared = [2];
    public function push(): void { array_push($this->items, 9); }
}
$b = new Box();
$b->push();
array_push(Box::$shared, 7);
$local = [1];
array_push($local, 4);
$nested = [[1]];
array_push($nested[0], 5);
echo count($b->items), count(Box::$shared), count($local), count($nested[0]);
"#,
    );
    assert_eq!(out, "3222");
}

/// The same growth requirement reached through a property receiver.
#[test]
fn test_array_unshift_on_instance_property_grows_before_prepending() {
    let out = compile_and_run(
        r#"<?php
class B { public $items = [1,2,3,0]; }
$b = new B();
array_unshift($b->items, 7);
rsort($b->items);
echo implode(",", $b->items);
"#,
    );
    assert_eq!(out, "7,3,2,1,0");
}
