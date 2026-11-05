//! Purpose:
//! Heap-debug coverage for concrete values bound to NULLABLE typed by-reference parameters
//! (`?array &$a`, `?Foo &$o`, `?float &$f`). An ordinary local takes the parameter's boxed cell,
//! so the callee may store `null` or any other admitted value like php; a DECLARED-type local
//! (`array<Item> $items = …`) keeps its typed slot and the boxed cell is written back after the
//! call, converting an array the callee built from Mixed values into the slot's element layout
//! and stopping the program when the value does not fit the declaration; a `null`-only local
//! gets boxed storage.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs under `--heap-debug` and asserts `leak summary: clean`, so a missed
//!   release of the previous occupant, the converted copy, or the parameter cell shows up.
//! - Expected stdout values follow php 8.5 for the same programs.

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
/// array the callee assigned, binds again for a second call, and nothing leaks across a loop.
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

/// Objects and floats bind the same way, and a replacement releases the previous value.
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

/// A callee that replaces a declared-type array argument with elements of another kind is
/// stopped with a diagnostic instead of corrupting the typed slot.
#[test]
fn test_nullable_array_reference_rejects_foreign_elements() {
    let out = compile_and_run_capture(
        r#"<?php
class Item {}
function corrupt(?array &$items): void { $items = ["not an item"]; }
function run(): void {
    array<Item> $items = [new Item()];
    corrupt($items);
    echo "unreachable";
}
run();
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

/// An object local bound to a nullable parameter of an ANCESTOR class may receive a sibling
/// instance (`?Base &$v` given an `A`, the callee stores a `B`): the local takes boxed storage
/// instead of keeping `A`'s layout, so it reads the sibling's own properties like php.
#[test]
fn test_nullable_ancestor_reference_accepts_sibling_object() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Base { public int $b = 1; }
class A extends Base { public int $a = 10; }
class B extends Base { public string $s = "bee"; }
function replace(?Base &$v): void { $v = new B(); }
function run(): void {
    $x = new A();
    replace($x);
    echo get_class($x), $x->b, $x->s, "\n";
}
run();
"#,
    );
    assert_clean(out, "B1bee\n");
}

/// A callee that replaces a declared-type object array with instances of an unrelated class is
/// stopped with a diagnostic instead of letting the slot be read at the wrong offsets.
#[test]
fn test_nullable_array_reference_rejects_foreign_object_elements() {
    let out = compile_and_run_capture(
        r#"<?php
class Item { public int $id = 1; }
class Other { public string $z = "zz"; }
function corrupt(?array &$items): void { $items = [new Other()]; }
function run(): void {
    array<Item> $items = [new Item()];
    corrupt($items);
    echo $items[0]->id, "unreachable";
}
run();
"#,
    );
    assert!(
        out.stderr.contains("replaced by an object of a class its typed variable cannot hold"),
        "stdout: {} stderr: {}",
        out.stdout,
        out.stderr
    );
    assert!(!out.stdout.contains("unreachable"), "stdout: {}", out.stdout);
}

/// A declared nested-array local bound to `?array &$a` keeps its typed layout when the callee leaves
/// nested arrays of the same element kind, and a nested replacement of another kind is
/// stopped with a diagnostic instead of being read with the slot's nested layout.
#[test]
fn test_nullable_array_reference_checks_nested_array_layouts() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function keep_tail(?array &$rows): void { $rows = [$rows[count($rows) - 1], [7, 8]]; }
function run(): void {
    array<array<int>> $rows = [[1, 2], [3, 4]];
    keep_tail($rows);
    echo count($rows), $rows[0][1], $rows[1][0], "\n";
}
run();
"#,
    );
    assert_clean(out, "247\n");
    let out = compile_and_run_capture(
        r#"<?php
function replace_nested(?array &$rows): void { $rows = [["x", "y"]]; }
function run(): void {
    array<array<int>> $rows = [[1, 2]];
    replace_nested($rows);
    echo $rows[0][1], "unreachable";
}
run();
"#,
    );
    assert!(
        out.stderr.contains("a by-reference argument was replaced by an array whose elements"),
        "stdout: {} stderr: {}",
        out.stdout,
        out.stderr
    );
    assert!(!out.stdout.contains("unreachable"), "stdout: {}", out.stdout);
}

/// A callee that stores `null` (or a value of another kind) through `?Foo &$o`, `?array &$a`
/// or `?float &$f` reaches an ordinary concrete local like php: the local takes the
/// parameter's boxed cell instead of a typed slot that could not hold the new value.
#[test]
fn test_nullable_reference_local_accepts_null_and_other_kinds() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box { public int $v = 1; }
function clear_box(?Box &$b): void { $b = null; }
function clear_list(?array &$a): void { $a = null; }
function clear_float(?float &$f): void { $f = null; }
function refill(?array &$a): void { $a = ["k" => "text", "n" => 2]; }
function run(): void {
    for ($i = 0; $i < 3; $i++) {
        $box = new Box();
        clear_box($box);
        $ints = [1, 2];
        clear_list($ints);
        $boxes = [new Box(), new Box()];
        clear_list($boxes);
        $f = 1.5;
        clear_float($f);
        $other = [new Box()];
        refill($other);
        if ($i == 2) {
            var_dump($box, $ints, $boxes, $f);
            echo $other["k"], $other["n"], "\n";
        }
    }
}
run();
"#,
    );
    assert_clean(out, "NULL\nNULL\nNULL\nNULL\ntext2\n");
}

/// A declared-type local keeps its typed slot, so a callee that stores `null` through
/// `?Box &$b` into `Box $box = …` is stopped with a diagnostic: the declaration does not admit
/// `null`, like a direct `$box = null;` would not.
#[test]
fn test_nullable_reference_declared_local_rejects_null() {
    let out = compile_and_run_capture(
        r#"<?php
class Box { public int $v = 1; }
function clear_box(?Box &$b): void { $b = null; }
function run(): void {
    Box $box = new Box();
    clear_box($box);
    echo "unreachable";
}
run();
"#,
    );
    assert!(
        out.stderr.contains("a by-reference argument was replaced by a value its typed variable cannot hold"),
        "stdout: {} stderr: {}",
        out.stdout,
        out.stderr
    );
    assert!(!out.stdout.contains("unreachable"), "stdout: {}", out.stdout);
}

/// The class check of a declared-type object array runs in a function that also calls `eval()`
/// (on another branch), so a foreign instance cannot be read with the declared class's layout.
#[test]
fn test_nullable_array_reference_checks_classes_beside_eval() {
    let out = compile_and_run_capture(
        r#"<?php
class Item { public int $id = 1; }
class Other { public string $z = "zz"; }
function corrupt(?array &$items): void { $items = [new Other()]; }
function run(bool $dynamic): void {
    if ($dynamic) {
        eval('echo "e";');
        return;
    }
    array<Item> $items = [new Item()];
    corrupt($items);
    echo $items[0]->id, "unreachable";
}
run(false);
"#,
    );
    assert!(
        out.stderr.contains("replaced by an object of a class its typed variable cannot hold"),
        "stdout: {} stderr: {}",
        out.stdout,
        out.stderr
    );
    assert!(!out.stdout.contains("unreachable"), "stdout: {}", out.stdout);
}

/// A declared nested-array local accepts nested arrays the callee rebuilt from `mixed` reads:
/// every level whose slots are boxed cells is converted into the declared layout (two and
/// three levels deep), the shared outer array is separated first, and nothing leaks.
#[test]
fn test_nullable_array_reference_converts_nested_mixed_arrays() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function rebuild(?array &$a): void { $a = [[$a[0][0]], [$a[1][1], 9]]; }
function deepen(?array &$a): void { $a = [[[$a[0][0][0] + 1, 7]], [[8]]]; }
function run(): void {
    for ($i = 0; $i < 3; $i++) {
        array<array<int>> $rows = [[1, 2], [3, 4]];
        rebuild($rows);
        array<array<array<int>>> $cube = [[[5]]];
        deepen($cube);
        if ($i == 2) {
            echo count($rows), $rows[0][0], $rows[1][0], $rows[1][1], "|";
            echo $cube[0][0][0], $cube[0][0][1], $cube[1][0][0], "\n";
        }
    }
}
run();
"#,
    );
    assert_clean(out, "2149|678\n");
}

/// A declared `array<array<mixed>>` local accepts nested arrays with raw typed slots: the
/// writeback widens each nested array into boxed cells instead of rejecting a value the
/// declaration admits, and nothing leaks.
#[test]
fn test_nullable_array_reference_widens_nested_arrays_into_mixed() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Cell { public function __construct(public int $v) {} }
function retype(?array &$a): void { $a = [[1, 2], ["x", "y"], [new Cell(5)]]; }
function run(): void {
    for ($i = 0; $i < 3; $i++) {
        array<array<mixed>> $rows = [[1, "a"]];
        retype($rows);
        if ($i == 2) {
            echo count($rows), $rows[0][0] + $rows[0][1], $rows[1][1], $rows[2][0]->v, "\n";
        }
    }
}
run();
"#,
    );
    assert_clean(out, "33y5\n");
}

/// A declared `array<callable>` local whose callee rebuilt the array from `mixed` reads keeps
/// its capturing closures alive: the converted array retains each callable descriptor, so the
/// closure still runs after the boxed copy is released, and nothing leaks.
#[test]
fn test_nullable_array_reference_retains_callables_on_conversion() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function rebuild(?array &$a): void { $a = [$a[0]]; }
function run(): void {
    for ($i = 0; $i < 3; $i++) {
        $n = $i;
        array<callable> $fns = [function () use ($n) { return $n * 10; }];
        rebuild($fns);
        if ($i == 2) {
            echo $fns[0](), "\n";
        }
    }
}
run();
"#,
    );
    assert_clean(out, "20\n");
}

/// A `mixed` local bound to a nullable object parameter (`?A &$v`) is checked against the
/// parameter like php: `null` and an `A` are accepted, a scalar or another class raises
/// `TypeError` before the callee runs.
#[test]
fn test_nullable_object_reference_rejects_foreign_boxed_values() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class A { public int $a = 1; }
class B {}
function src(int $k): mixed { return $k === 0 ? 7 : ($k === 1 ? new B() : ($k === 2 ? null : new A())); }
function take(?A &$v): string { return $v === null ? "null" : get_class($v); }
function run(): void {
    foreach ([0, 1, 2, 3] as $k) {
        $v = src($k);
        try {
            echo take($v), "|";
        } catch (TypeError $e) {
            echo "TypeError|";
        }
    }
    echo "\n";
}
run();
"#,
    );
    assert_clean(out, "TypeError|TypeError|null|A|\n");
}
