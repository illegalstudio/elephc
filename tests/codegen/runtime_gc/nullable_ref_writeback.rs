//! Purpose:
//! Heap-debug coverage for concrete locals bound to NULLABLE typed by-reference parameters
//! (`?array &$a`, `?Foo &$o`, `?float &$f`): the boxed parameter cell is written back into the
//! typed slot after the call, converting an array the callee built from Mixed values into the
//! local's element layout, and a `null`-only local gets boxed storage.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs under `--heap-debug` and asserts `leak summary: clean`, so a missed
//!   release of the previous occupant, the converted copy, or the parameter cell shows up.
//! - Expected stdout values follow php 8.4 for the same programs.

use crate::support::{compile_and_run_capture, compile_and_run_with_heap_debug};

/// Asserts the program printed `expected` and left a clean heap under heap debug.
fn assert_clean(out: crate::support::ProgramOutput, expected: &str) {
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// An `array<Foo>` / `array<int>` / `array<string>` local passed to `?array &$a` receives the
/// array the callee assigned (built from Mixed values, so its slots are converted), keeps its
/// typed shape for a second call, and nothing leaks across a loop.
#[test]
fn test_nullable_array_reference_writes_back_typed_arrays() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Item { public function __construct(public int $id) {} }
function keep_last(?array &$items): int {
    if ($items === null) { return 0; }
    $items = [$items[count($items) - 1]];
    return 1;
}
function run(): void {
    for ($i = 0; $i < 3; $i++) {
        $objects = [new Item(1), new Item(2), new Item(3)];
        keep_last($objects);
        keep_last($objects);
        $ints = [5, 6, 7];
        keep_last($ints);
        $strings = ["a", "b"];
        keep_last($strings);
        if ($i == 2) {
            echo count($objects), $objects[0]->id, "|", $ints[0], "|", $strings[0], "\n";
        }
    }
}
run();
"#,
    );
    assert_clean(out, "13|7|b\n");
}

/// A local that only ever holds `null` binds to a nullable by-reference parameter (it gets boxed
/// storage), and a callee that leaves `null` alone keeps it `null` — php's `$read = null;
/// mysqli_poll($read, …)` shape.
#[test]
fn test_null_local_binds_to_nullable_reference_parameters() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function maybe_fill(?array &$out, bool $fill): void {
    if ($fill) { $out = [1, 2]; }
}
function run(): void {
    $untouched = null;
    maybe_fill($untouched, false);
    var_dump($untouched);
    $filled = null;
    maybe_fill($filled, true);
    echo count($filled), "\n";
}
run();
"#,
    );
    assert_clean(out, "NULL\n2\n");
}

/// Objects and floats ride the same writeback, and a replacement releases the previous value.
#[test]
fn test_nullable_object_and_float_references_write_back() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box { public function __construct(public int $v) {} }
function swap(?Box &$box): void { $box = new Box($box === null ? 0 : $box->v + 1); }
function twice(?float &$f): void { $f = $f * 2; }
function run(): void {
    $b = new Box(4);
    swap($b);
    swap($b);
    $f = 1.25;
    twice($f);
    echo $b->v, "|", $f, "\n";
}
run();
"#,
    );
    assert_clean(out, "6|2.5\n");
}

/// A callee that replaces a typed array argument with elements of another kind is stopped with
/// a diagnostic instead of corrupting the typed slot.
#[test]
fn test_nullable_array_reference_rejects_foreign_elements() {
    let out = compile_and_run_capture(
        r#"<?php
class Item {}
function corrupt(?array &$items): void { $items = ["not an item"]; }
$items = [new Item()];
corrupt($items);
echo "unreachable";
"#,
    );
    assert!(
        out.stderr.contains("a by-reference argument was replaced by an array whose elements"),
        "stderr: {}",
        out.stderr
    );
}

/// An `array<mixed>` local (a heterogeneous literal, or a list rebuilt in a loop) passed to
/// `?array &$a` accepts a callee array with raw typed slots: the writeback widens it into boxed
/// cells, and the boxed array the callee keeps is retained as it is.
#[test]
fn test_nullable_array_reference_widens_into_mixed_locals() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Item { public function __construct(public int $id) {} }
function replace_items(?array &$items, int $mode): void {
    if ($mode === 0) {
        $items = [new Item(4), new Item(5)];
    } elseif ($mode === 1) {
        $items = [8, 9, 10];
    } else {
        $items[] = "tail";
    }
}
function run(): void {
    for ($i = 0; $i < 3; $i++) {
        $values = [1, "a", 2.5];
        replace_items($values, 0);
        $first = $values[0];
        $other = [true, "b"];
        replace_items($other, 1);
        $kept = [1, "x"];
        replace_items($kept, 2);
        if ($i == 2) {
            echo count($values), $first->id, "|", count($other), $other[2], "|", count($kept), $kept[2], "\n";
        }
    }
}
run();
"#,
    );
    assert_clean(out, "24|310|3tail\n");
}
