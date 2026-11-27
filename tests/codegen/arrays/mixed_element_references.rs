//! Purpose:
//! Regression tests for element references taken through a BOXED receiver: a `mixed`
//! parameter or local, or a declared `array` parameter (boxed on this branch so its
//! representation can change without retyping the caller's slot).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - PHP separates an array the moment one of its elements becomes a reference, so an alias
//!   taken BEFORE `$r = &$a[...]` must keep the original value. The boxed lowering rewrote the
//!   shared cell in place and every alias saw the reference and the writes through it.
//! - The packed case additionally promotes the payload to a hash; the promoted hash is now
//!   published into the cell before the packed owner is released, so heap debug stays clean
//!   and nothing observes the cell pointing at freed storage.
//! - Every expected value is real `php` 8.x output for the same fixture.

use crate::support::*;

/// An alias of a `mixed` hash must not observe a reference taken through the original.
#[test]
fn test_element_reference_through_mixed_hash_separates_the_alias() {
    let out = compile_and_run(
        r#"<?php
function f(mixed $a): void {
    $b = $a;
    $r = &$a["x"];
    $r = 2;
    echo $b["x"], "|", $a["x"], "|", count($b), "|", count($a), "\n";
}
f(["x" => 1]);
"#,
    );
    assert_eq!(out, "1|2|1|1\n");
}

/// The packed spelling: the reference promotes the payload to a hash, which must land in a
/// cell the alias does not share.
#[test]
fn test_element_reference_through_mixed_packed_array_separates_the_alias() {
    let out = compile_and_run(
        r#"<?php
function g(mixed $a): void {
    $b = $a;
    $r = &$a[0];
    $r = 9;
    echo $b[0], "|", $a[0], "|", count($b), "|", count($a), "\n";
}
g([1, 2]);
"#,
    );
    assert_eq!(out, "1|9|2|2\n");
}

/// A declared `array` parameter is boxed too, by value and by reference; both must separate
/// from a local alias, and the by-reference form must still reach the caller.
#[test]
fn test_element_reference_through_declared_array_parameters_separates_the_alias() {
    let out = compile_and_run(
        r#"<?php
function h(array $a): void { $b = $a; $r = &$a["x"]; $r = 2; echo $b["x"], "|", $a["x"], "\n"; }
h(["x" => 1]);
function p(array $a): void { $b = $a; $r = &$a[0]; $r = 9; echo $b[0], "|", $a[0], "\n"; }
p([1, 2]);
function rf(array &$a): void { $b = $a; $r = &$a["x"]; $r = 2; echo $b["x"], "|", $a["x"], "\n"; }
$q = ["x" => 1]; rf($q); echo $q["x"], "\n";
"#,
    );
    assert_eq!(out, "1|2\n1|9\n1|2\n2\n");
}

/// `array_shift()` and `array_unshift()` preserve the surviving reference cells.
#[test]
fn test_reference_cells_survive_shift_and_unshift() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
$r = &$a[1];
$shifted = array_shift($a);
$r = 99;
echo $shifted, "|", $a[0], "|", $a[1], "\n";
$b = [1, 2];
$s = &$b[0];
array_unshift($b, 0);
$s = 42;
echo $b[0], "|", $b[1], "|", $b[2], "\n";
$c = [1, 2, 3];
$t = &$c[0];
$reversed = array_reverse($c);
$t = 7;
echo $reversed[2], "|", $reversed[0], "\n";
$d = [1, 2];
$u = &$d[0];
array_unshift($d);
$u = 5;
echo $d[0], "\n";
$f = [1, 2];
$x = &$f[1];
$merged = array_merge($f, [3]);
$x = 9;
echo $merged[1], "\n";
"#,
    );
    assert_eq!(out, "1|99|3\n0|42|2\n7|3\n5\n9\n");
}

/// Reference array literals create undefined variables silently, like PHP.
#[test]
fn test_reference_literals_create_undefined_variables_silently() {
    let output = compile_and_run_capture(
        r#"<?php
set_error_handler(function (int $errno, string $message): bool {
    echo "WARN:", $message, "\n";
    return true;
});
$refs = [&$third];
var_dump($third);
"#,
    );
    assert_eq!(output.stdout, "NULL\n");
    assert!(
        !output.stderr.contains("Undefined variable"),
        "stderr: {:?}",
        output.stderr
    );
}

/// A receiver that shares nothing must not be copied: the reference still reaches the only
/// cell, a missing key is created in it, and a non-array `mixed` keeps answering no element.
#[test]
fn test_element_reference_through_unshared_mixed_keeps_the_single_cell() {
    let out = compile_and_run(
        r#"<?php
function f(mixed $a): void {
    $r = &$a["new"];
    $r = 5;
    echo count($a), "|", $a["new"], "\n";
}
f(["x" => 1]);
function n(mixed $a): void {
    $r = &$a["x"];
    echo var_export($r, true), "\n";
}
n(null);
"#,
    );
    assert_eq!(out, "2|5\nNULL\n");
}

/// The separation adds one cell and the promotion replaces one payload; both must be balanced
/// on every receiver kind, including the by-reference parameter whose store-back retires the
/// previous cell.
#[test]
fn test_element_reference_through_boxed_receivers_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f(mixed $a): int { $b = $a; $r = &$a["x"]; $r = 2; return $b["x"] + $a["x"]; }
function g(mixed $a): int { $b = $a; $r = &$a[0]; $r = 9; return $b[0] + $a[0]; }
function rf(array &$a): void { $b = $a; $r = &$a[1]; $r = 7; }
$t = 0;
for ($i = 0; $i < 3; $i++) {
    $t += f(["x" => 1]) + g([1, 2]);
    $q = [1, 2, 3]; rf($q); $t += $q[1];
}
echo $t, "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "60\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A reference-element literal must describe its hash-of-Mixed storage. Before the literal
/// widened to `AssocArray{Mixed, Mixed}`, `array_values()` inferred `array<int>` and printed
/// the boxed cell pointer as an integer. elephc extracts the aliased value instead of the
/// reference (PHP keeps the cell and prints `&int(2)`), so the snapshot keeps the value the
/// alias held when the array was read; the corruption this guards against printed a pointer.
#[test]
fn test_array_values_over_reference_element_literal_keeps_the_scalar_payload() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$a = [&$x];
$b = array_values($a);
$x = 2;
var_dump($b);
"#,
    );
    assert_eq!(out, "array(1) {\n  [0]=>\n  int(1)\n}\n");
}

/// The associative literal spelling widens the same way.
#[test]
fn test_array_values_over_assoc_reference_element_literal_keeps_the_scalar_payload() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$a = ["k" => &$x];
$b = array_values($a);
var_dump($b);
"#,
    );
    assert_eq!(out, "array(1) {\n  [0]=>\n  int(1)\n}\n");
}

/// `sort()`/`rsort()` relink a reference-shaped hash and renumber its keys without detaching
/// the managed cells: later writes through either alias must reach the reordered array.
#[test]
fn test_sort_and_rsort_preserve_reference_cells() {
    let out = compile_and_run(
        r#"<?php
$x = 1; $y = 2;
$a = [&$y, &$x];
sort($a);
$x = 9;
var_dump($a);

$p = 1; $q = 2;
$b = [&$q, &$p];
rsort($b);
$p = 9;
var_dump($b);
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [0]=>\n  &int(9)\n  [1]=>\n  &int(2)\n}\n\
array(2) {\n  [0]=>\n  &int(2)\n  [1]=>\n  &int(9)\n}\n"
    );
}

/// A hash copy taken before the sort keeps its own order and its reference cells.
#[test]
fn test_sort_reference_cells_leave_a_copy_unsorted() {
    let out = compile_and_run(
        r#"<?php
$x = 1; $y = 2;
$a = [&$y, &$x];
$copy = $a;
sort($a);
$x = 9;
var_dump($a);
var_dump($copy);
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [0]=>\n  &int(9)\n  [1]=>\n  &int(2)\n}\n\
array(2) {\n  [0]=>\n  &int(2)\n  [1]=>\n  &int(9)\n}\n"
    );
}

/// A declared `array` by-reference parameter boxes its payload, so the sort must promote the
/// hash in place instead of extracting and detaching its values.
#[test]
fn test_sort_reference_cells_through_by_ref_array_parameter() {
    let out = compile_and_run(
        r#"<?php
function sortRefs(array &$a): void { sort($a); }
$x = 1; $y = 2;
$a = [&$y, &$x];
sortRefs($a);
$x = 9;
var_dump($a);
"#,
    );
    assert_eq!(out, "array(2) {\n  [0]=>\n  &int(9)\n  [1]=>\n  &int(2)\n}\n");
}

/// The same applies to a declared array property, which is also a boxed receiver.
#[test]
fn test_sort_reference_cells_through_declared_array_property() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class RefSortBox { public array $items = []; }
$x = 1; $y = 2;
$b = new RefSortBox();
$b->items = [&$y, &$x];
sort($b->items);
$x = 9;
var_dump($b->items);
unset($b);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "array(2) {\n  [0]=>\n  &int(9)\n  [1]=>\n  &int(2)\n}\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Mixed-valued hashes without references keep PHP's ordering and dense renumbering through
/// the new relink-and-reindex path.
#[test]
fn test_sort_mixed_valued_hash_without_references() {
    let out = compile_and_run(
        r#"<?php
$a = ["x" => "b", "y" => "a", "z" => 2];
sort($a);
var_dump($a);
$b = [5 => "b", 9 => "a", 2 => "c"];
rsort($b);
var_dump($b);
"#,
    );
    assert_eq!(
        out,
        "array(3) {\n  [0]=>\n  int(2)\n  [1]=>\n  string(1) \"a\"\n  [2]=>\n  string(1) \"b\"\n}\n\
array(3) {\n  [0]=>\n  string(1) \"c\"\n  [1]=>\n  string(1) \"b\"\n  [2]=>\n  string(1) \"a\"\n}\n"
    );
}

/// A reference element written after `unset($a)` re-creates the variable instead of widening
/// the killed slot's null storage into a boxed cell.
#[test]
fn test_reference_element_after_unset_recreates_the_container() {
    let out = compile_and_run(
        r#"<?php
$x = 7;
$a = [1, 2];
unset($a);
$a["k"] = &$x;
$x = 9;
var_dump($a);

function recreate(): void {
    $y = 1;
    $b = ["old" => 2];
    unset($b);
    $b[0] = &$y;
    $y = 5;
    var_dump($b);
}
recreate();
"#,
    );
    assert_eq!(
        out,
        "array(1) {\n  [\"k\"]=>\n  &int(9)\n}\narray(1) {\n  [0]=>\n  &int(5)\n}\n"
    );
}

/// A null binding auto-vivifies the same way, and appends after unset keep their keys.
#[test]
fn test_reference_element_after_null_and_append_after_unset() {
    let out = compile_and_run(
        r#"<?php
$x = 3;
$a = null;
$a["k"] = &$x;
$x = 4;
var_dump($a);

$b = [1, 2];
unset($b);
$b[] = 7;
$b[] = 8;
var_dump($b);
"#,
    );
    assert_eq!(
        out,
        "array(1) {\n  [\"k\"]=>\n  &int(4)\n}\narray(2) {\n  [0]=>\n  int(7)\n  [1]=>\n  int(8)\n}\n"
    );
}

/// Replacing the killed slot must release the unset null box and the new hash exactly once.
#[test]
fn test_reference_element_after_unset_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function recreate(int $round): int {
    $x = $round;
    $a = ["old" => $round];
    unset($a);
    $a["k"] = &$x;
    $x = $x + 1;
    return $a["k"];
}
$total = 0;
for ($i = 0; $i < 4; $i++) {
    $total += recreate($i);
}
echo $total, "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "10\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// `shuffle()` permutes a reference-shaped hash's chain and renumbers its keys without moving
/// or detaching the managed cells.
#[test]
fn test_shuffle_preserves_reference_cells() {
    let out = compile_and_run(
        r#"<?php
$x = 1; $y = 2; $z = 3;
$a = [&$x, &$y, &$z];
shuffle($a);
sort($a);
var_dump($a);
$x = 10; $y = 20; $z = 30;
var_dump($a);
"#,
    );
    assert_eq!(
        out,
        "array(3) {\n  [0]=>\n  &int(1)\n  [1]=>\n  &int(2)\n  [2]=>\n  &int(3)\n}\n\
array(3) {\n  [0]=>\n  &int(10)\n  [1]=>\n  &int(20)\n  [2]=>\n  &int(30)\n}\n"
    );
}

/// A hash copy keeps its order through a shuffle of the original, and a mixed-valued hash
/// without references shuffles like PHP.
#[test]
fn test_shuffle_hash_keeps_a_copy_and_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["b" => 2, "a" => 1, "c" => 3];
$copy = $a;
shuffle($a);
sort($a);
var_dump($a);
sort($copy);
var_dump($copy);
unset($a, $copy);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "array(3) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n  [2]=>\n  int(3)\n}\n\
array(3) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n  [2]=>\n  int(3)\n}\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// A loop body is lowered once, so an array write after `null`/`unset()` must keep reading the
/// container the previous iteration published instead of allocating a fresh one each time.
#[test]
fn test_array_writes_after_null_accumulate_in_a_loop() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = null;
$i = 0;
while ($i < 3) { $a[] = $i; ++$i; }
var_dump($a);

$b = null;
$i = 0;
while ($i < 3) { $b["k$i"] = $i; ++$i; }
var_dump($b);

$c = null;
$x = 9;
$i = 0;
while ($i < 3) { $c["k$i"] = &$x; ++$i; }
$x = 7;
var_dump($c);

function unsetThenAppend(): void {
    $d = [1, 2];
    unset($d);
    $i = 0;
    while ($i < 2) { $d[] = $i; ++$i; }
    var_dump($d);
}
unsetThenAppend();
unset($a, $b, $c, $x);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "array(3) {\n  [0]=>\n  int(0)\n  [1]=>\n  int(1)\n  [2]=>\n  int(2)\n}\n\
array(3) {\n  [\"k0\"]=>\n  int(0)\n  [\"k1\"]=>\n  int(1)\n  [\"k2\"]=>\n  int(2)\n}\n\
array(3) {\n  [\"k0\"]=>\n  &int(7)\n  [\"k1\"]=>\n  &int(7)\n  [\"k2\"]=>\n  &int(7)\n}\n\
array(2) {\n  [0]=>\n  int(0)\n  [1]=>\n  int(1)\n}\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Nested arrays in a reference-promoted hash still fail closed before the sort mutates it.
#[test]
fn test_sort_reference_hash_rejects_non_scalar_values() {
    let error = compile_and_run_expect_failure(
        r#"<?php
$x = [2]; $y = [1];
$a = [&$x, &$y];
sort($a);
"#,
    );
    assert!(
        error.contains("sorting Mixed arrays containing non-scalar values is not supported"),
        "unexpected runtime diagnostic: {error}"
    );
}

/// Inline `[&$x]` callback arguments bind the pushed entry to the source variable.
#[test]
fn test_call_user_func_array_inline_reference_argument() {
    let out = compile_and_run(
        r#"<?php
$set = function (&$v) { $v = 2; };
$x = 1;
call_user_func_array($set, [&$x]);
echo $x, "|";
$y = 3;
$args = [&$y];
call_user_func_array($set, $args);
echo $y;
"#,
    );
    assert_eq!(out, "2|2");
}

/// A duplicate literal key replaces an earlier reference entry instead of writing through it.
#[test]
fn test_assoc_literal_duplicate_key_replaces_reference_binding() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$a = ['k' => &$x, 'k' => 2];
echo $x, "|", $a['k'], "|";
$y = 1;
$z = 2;
$b = ['k' => &$y, 'k' => &$z];
$b['k'] = 9;
echo $y, "|", $z, "|", $b['k'];
"#,
    );
    assert_eq!(out, "1|2|1|9|9");
}
