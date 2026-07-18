//! Purpose:
//! Integration tests for heterogeneous indexed arrays backed by boxed Mixed slots.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - These fixtures cover literal construction, widening writes, foreach, COW,
//!   and mutating builtin append paths.

use crate::support::*;

/// Verifies that a literal heterogeneous array (int, string, bool, float)
/// can be constructed and each element accessed by index with correct
/// PHP string conversion (bool `true` becomes `"1"`).
#[test]
fn test_heterogeneous_indexed_array_literal_access() {
    let out = compile_and_run(
        r#"<?php
$items = [1, "two", true, 3.5];
echo $items[0] . "|" . $items[1] . "|" . $items[2] . "|" . $items[3];
"#,
    );
    assert_eq!(out, "1|two|1|3.5");
}

/// Verifies that appending a string via `[]` widens a previously integer-typed
/// slot to string, and that `gettype()` reflects the new runtime type.
#[test]
fn test_heterogeneous_indexed_array_push_widens_existing_slots() {
    let out = compile_and_run(
        r#"<?php
$items = [1];
$items[] = "two";
echo gettype($items[0]) . "|" . $items[0] . "|" . gettype($items[1]) . "|" . $items[1];
"#,
    );
    assert_eq!(out, "integer|1|string|two");
}

/// Verifies that assigning a string to an integer-typed indexed slot widens
/// the slot to string while leaving the first element untouched.
#[test]
fn test_heterogeneous_indexed_array_assignment_widens_existing_slots() {
    let out = compile_and_run(
        r#"<?php
$items = [1, 2];
$items[1] = "two";
echo $items[0] . "|" . $items[1];
"#,
    );
    assert_eq!(out, "1|two");
}

/// Verifies COW semantics: copying an array, then appending to the copy does not
/// mutate the original. Counts must reflect independent lengths and values.
#[test]
fn test_heterogeneous_indexed_array_copy_on_write() {
    let out = compile_and_run(
        r#"<?php
$left = [1];
$right = $left;
$right[] = "two";
echo count($left) . "|" . count($right) . "|" . $left[0] . "|" . $right[1];
"#,
    );
    assert_eq!(out, "1|2|1|two");
}

/// Verifies that foreach over a heterogeneous array yields each value in
/// source order, with PHP string conversion applied to each element.
#[test]
fn test_heterogeneous_indexed_array_foreach_values() {
    let out = compile_and_run(
        r#"<?php
$items = [1, "two", 3];
foreach ($items as $value) {
    echo $value . "|";
}
"#,
    );
    assert_eq!(out, "1|two|3|");
}

/// Verifies that a nested array literal within a heterogeneous array can be
/// accessed via chained index notation (`$items[0][0]`).
#[test]
fn test_heterogeneous_indexed_array_nested_typed_array_access() {
    let out = compile_and_run(
        r#"<?php
$items = [[10, 20], 30];
echo $items[0][0] . "|" . $items[0][1] . "|" . $items[1];
"#,
    );
    assert_eq!(out, "10|20|30");
}

/// Verifies that `array_push` appends a string element to a heterogeneous
/// array and the result is accessible by index.
#[test]
fn test_heterogeneous_indexed_array_push_builtin() {
    let out = compile_and_run(
        r#"<?php
$items = [1];
array_push($items, "two");
echo $items[0] . "|" . $items[1];
"#,
    );
    assert_eq!(out, "1|two");
}

/// Verifies that widening an integer slot to string via `[]` append does not
/// leak: net allocations minus baseline equal net frees after unset.
#[test]
fn test_heterogeneous_indexed_array_push_balances_gc_stats() {
    let baseline = compile_and_run_with_gc_stats("<?php");
    let out = compile_and_run_with_gc_stats(
        r#"<?php
$items = [1];
$items[] = "two";
unset($items);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs - baseline_allocs, frees - baseline_frees);
}

/// Verifies that `array_push` with a widening string element does not leak:
/// net allocations minus baseline equal net frees after unset.
#[test]
fn test_heterogeneous_indexed_array_push_builtin_balances_gc_stats() {
    let baseline = compile_and_run_with_gc_stats("<?php");
    let out = compile_and_run_with_gc_stats(
        r#"<?php
$items = [1];
array_push($items, "two");
unset($items);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs - baseline_allocs, frees - baseline_frees);
}

/// Verifies that a nested array literal `[[2]]` inside a heterogeneous array
/// does not leak: net allocations minus baseline equal net frees after unset.
#[test]
fn test_heterogeneous_indexed_array_nested_literal_balances_gc_stats() {
    let baseline = compile_and_run_with_gc_stats("<?php");
    let out = compile_and_run_with_gc_stats(
        r#"<?php
$items = [1, [2]];
unset($items);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs - baseline_allocs, frees - baseline_frees);
}

/// Regression test: repeatedly appending integers to a fresh array after
/// cycling through a string-seeding pattern must not retain a string shape
/// that causes `str_repeat` interning or heap corruption. Uses heap debug
/// mode to catch misuse of string payload pointers.
#[test]
fn test_empty_array_int_pushes_do_not_retain_string_shape() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($i = 0; $i < 20; $i++) {
    $seed = [];
    $seed[] = str_repeat("x", 32);
    unset($seed);

    $poll_map = [];
    for ($j = 0; $j < 64; $j++) {
        $poll_map[] = $j;
    }
    unset($poll_map);
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// An indexed array-literal element that is a `$this->prop->method()` call — a property-access
/// receiver, not `$this` — must be typed by the callee's declared return type. Before the fix,
/// `instance_callable_object_class` resolved Variable/This/new/function receivers but not a
/// property-access receiver, so `[$this->factory->link(...)]` fell to the syntactic `Int` default
/// and int-cast the returned object ("int cast for Object(<x>)"). This exercises both the
/// `array_literal_element_type_for_ir` method-call arm and the property-access receiver resolution.
#[test]
fn test_indexed_array_literal_property_receiver_method_element_typed_by_return() {
    let out = compile_and_run(
        r#"<?php
declare(strict_types=1);
final class Link { public function __construct(public string $label) {} }
final class Factory { public function link(string $l): Link { return new Link($l); } }
final class Row { public function __construct(public array $links) {} }
final class Composer {
    public function __construct(private Factory $factory) {}
    public function row(): Row { return new Row([$this->factory->link('View'), $this->factory->link('Edit')]); }
}
$c = new Composer(new Factory());
echo $c->row()->links[0]->label, '|', $c->row()->links[1]->label;
"#,
    );
    assert_eq!(out, "View|Edit");
}
