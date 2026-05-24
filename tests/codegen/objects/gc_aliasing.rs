//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object object GC aliasing, including GC array alias survives unset, GC returned array alias survives caller unset, and GC returned object alias survives caller unset.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
// Verifies that a copy-on-write array alias survives unset of the original variable.
// Fixture: two variables share a 3-element array; unset the first; read from the second.
// Regression: ensures GC does not collect the shared backing store prematurely.
fn test_gc_array_alias_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$b = $a;
unset($a);
echo $b[0];
echo $b[1];
echo $b[2];
"#,
    );
    assert_eq!(out, "102030");
}

#[test]
// Verifies that a returned array alias survives caller-side unset of the original variable.
// Fixture: pass an array to a function that returns it; unset the caller's copy; read the returned alias.
// Regression: ensures callee-owned array backing store survives caller-side unset.
fn test_gc_returned_array_alias_survives_caller_unset() {
    let out = compile_and_run(
        r#"<?php
function share($arr) {
    return $arr;
}

$a = [7, 8];
$b = share($a);
unset($a);
echo $b[0];
echo $b[1];
"#,
    );
    assert_eq!(out, "78");
}

#[test]
// Verifies that a returned object alias survives caller-side unset of the original variable.
// Fixture: pass an object to a function that returns it; unset the caller's copy; read the returned alias's property.
// Regression: ensures GC-preserved object identity is maintained through caller-side unset.
fn test_gc_returned_object_alias_survives_caller_unset() {
    let out = compile_and_run(
        r#"<?php
class Box { public $val = 0; }

function share($box) {
    return $box;
}

$a = new Box();
$a->val = 41;
$b = share($a);
unset($a);
echo $b->val;
"#,
    );
    assert_eq!(out, "41");
}

#[test]
// Verifies that an inner array pushed into an outer array survives unset of the original.
// Fixture: create inner array, push it into outer array, unset inner, read from outer.
// Regression: ensures push-path does not break GC alias on nested array.
fn test_gc_array_push_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [9];
$outer = [];
$outer[] = $inner;
unset($inner);
echo $outer[0][0];
"#,
    );
    assert_eq!(out, "9");
}

#[test]
// Verifies that an inner array in an array literal survives unset of the original.
// Fixture: assign inner array to indexed position in outer array literal, unset inner, read from outer.
// Regression: ensures array literal initialization preserves GC alias.
fn test_gc_indexed_array_literal_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [3, 4];
$outer = [$inner];
unset($inner);
echo $outer[0][1];
"#,
    );
    assert_eq!(out, "4");
}

#[test]
// Verifies that an inner array assigned by index to an outer array survives unset of the original.
// Fixture: create outer array with literal elements, assign inner array to a specific index, unset inner, read from outer.
// Regression: ensures index-based assignment preserves GC alias for nested arrays.
fn test_gc_array_assign_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [4];
$outer = [[1], [2]];
$outer[1] = $inner;
unset($inner);
echo $outer[1][0];
"#,
    );
    assert_eq!(out, "4");
}

#[test]
// Verifies that an inner array assigned to an object property survives unset of the original.
// Fixture: create Holder object, assign inner array to property, unset inner, read from property.
// Regression: ensures property store preserves GC alias for nested arrays.
fn test_gc_property_assign_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
class Holder { public $value; }

$inner = [7];
$h = new Holder();
$h->value = $inner;
unset($inner);
$saved = $h->value;
echo $saved[0];
"#,
    );
    assert_eq!(out, "7");
}

#[test]
// Verifies that a static variable that receives a borrowed inner array survives unset of the original.
// Fixture: assign inner array to static variable, unset temp, read from static.
// Regression: ensures static storage preserves GC alias across unset of source.
fn test_gc_static_assign_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
function hold_once() {
    static $saved = [];
    $tmp = [5];
    $saved = $tmp;
    unset($tmp);
    echo $saved[0];
}

hold_once();
"#,
    );
    assert_eq!(out, "5");
}

#[test]
// Verifies that an inner array spread into a new destination array survives unset of the original.
// Fixture: nest inner in src, spread src into dst, unset src and inner, read from dst.
// Regression: ensures spread creates a COW copy that preserves inner alias after unset.
fn test_gc_spread_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [8];
$src = [$inner];
$dst = [...$src];
unset($src);
unset($inner);
echo $dst[0][0];
"#,
    );
    assert_eq!(out, "8");
}

#[test]
// Verifies that an inner array merged via array_merge survives unset of the original.
// Fixture: left array contains inner array, right array contains separate element, merge, unset left and inner, read merged result.
// Regression: ensures array_merge does not break GC alias for nested inner arrays.
fn test_gc_array_merge_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [6];
$left = [$inner];
$right = [[7]];
$merged = array_merge($left, $right);
unset($left);
unset($inner);
echo $merged[0][0] . "|" . $merged[1][0];
"#,
    );
    assert_eq!(out, "6|7");
}

#[test]
// Verifies that an inner array in an array_chunk result survives unset of the source.
// Fixture: create rows array with inner array, chunk it, unset rows and inner, read from first chunk element.
// Regression: ensures array_chunk output preserves GC alias for nested inner arrays.
fn test_gc_array_chunk_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [5];
$rows = [$inner, [9]];
$chunks = array_chunk($rows, 1);
unset($rows);
unset($inner);
echo $chunks[0][0][0] . "|" . $chunks[1][0][0];
"#,
    );
    assert_eq!(out, "5|9");
}

#[test]
// Verifies that an inner array extracted via array_slice survives unset of the source.
// Fixture: src contains inner array at index 1 among other elements, slice index 1 length 1, unset src and inner, read from slice.
// Regression: ensures array_slice output preserves GC alias for nested inner arrays.
fn test_gc_array_slice_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [2];
$src = [[1], $inner, [3]];
$slice = array_slice($src, 1, 1);
unset($src);
unset($inner);
echo $slice[0][0];
"#,
    );
    assert_eq!(out, "2");
}

#[test]
// Verifies that an inner array in an array_reverse result survives unset of the source.
// Fixture: src contains inner array at index 1 among other elements, reverse src, unset src and inner, read from reversed index 1.
// Regression: ensures array_reverse output preserves GC alias for nested inner arrays.
fn test_gc_array_reverse_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [4];
$src = [[1], $inner, [7]];
$rev = array_reverse($src);
unset($src);
unset($inner);
echo $rev[1][0];
"#,
    );
    assert_eq!(out, "4");
}

#[test]
// Verifies that an inner array used as array_pad fill value survives unset of the original.
// Fixture: create src with one element, pad to length 3 using inner as fill value, unset src and inner, read padded indices 1 and 2.
// Regression: ensures array_pad fill-value path preserves GC alias.
fn test_gc_array_pad_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [5];
$src = [[1]];
$padded = array_pad($src, 3, $inner);
unset($src);
unset($inner);
echo $padded[1][0] . "|" . $padded[2][0];
"#,
    );
    assert_eq!(out, "5|5");
}

#[test]
// Verifies that an inner array in array_unique output survives unset of the original.
// Fixture: src contains inner array twice and a separate element, run array_unique, unset src and inner, verify count and read values.
// Regression: ensures array_unique preserves GC alias for deduplicated inner arrays.
fn test_gc_array_unique_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [3];
$src = [$inner, $inner, [4]];
$uniq = array_unique($src);
unset($src);
unset($inner);
echo count($uniq) . "|" . $uniq[0][0] . "|" . $uniq[1][0];
"#,
    );
    assert_eq!(out, "2|3|4");
}

#[test]
// Verifies that an inner array extracted via array_splice (removed portion) survives unset of the source.
// Fixture: src contains inner array at index 1 among other elements, splice out index 1, unset src and inner, read removed portion.
// Regression: ensures array_splice removed portion preserves GC alias for nested inner arrays.
fn test_gc_array_splice_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [7];
$src = [[1], $inner, [9]];
$removed = array_splice($src, 1, 1);
unset($src);
unset($inner);
echo $removed[0][0];
"#,
    );
    assert_eq!(out, "7");
}

#[test]
// Verifies that an inner array in array_diff output survives unset of the original.
// Fixture: left contains inner array and another element, right contains only the other element, diff, unset left and inner, read result.
// Regression: ensures array_diff output preserves GC alias for nested inner arrays.
fn test_gc_array_diff_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [6];
$left = [$inner, [8]];
$right = [[8]];
$diff = array_diff($left, $right);
unset($left);
unset($inner);
echo $diff[0][0];
"#,
    );
    assert_eq!(out, "6");
}

#[test]
// Verifies that an inner array in array_intersect output survives unset of the originals.
// Fixture: left contains inner array and another element, right contains inner array, intersect, unset all, read result.
// Regression: ensures array_intersect output preserves GC alias for nested inner arrays.
fn test_gc_array_intersect_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [9];
$left = [[1], $inner];
$right = [$inner];
$both = array_intersect($left, $right);
unset($left);
unset($right);
unset($inner);
echo $both[0][0];
"#,
    );
    assert_eq!(out, "9");
}

#[test]
// Verifies that an inner array in array_filter output survives unset of the original.
// Fixture: rows contain inner array among other elements, filter by a callback that keeps 2-element arrays, unset rows and inner, read filtered result.
// Regression: ensures array_filter output preserves GC alias for nested inner arrays.
fn test_gc_array_filter_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
function keep_pair($x) { return count($x) == 2; }
$inner = [10, 11];
$rows = [[1], $inner, [2, 3]];
$filtered = array_filter($rows, "keep_pair");
unset($rows);
unset($inner);
echo $filtered[0][1] . "|" . $filtered[1][0];
"#,
    );
    assert_eq!(out, "11|2");
}

#[test]
// Verifies that an inner array used as array_fill fill value survives unset of the original.
// Fixture: create inner array, fill an array of length 2 with inner as fill value, unset inner, read both fill positions.
// Regression: ensures array_fill fill-value path preserves GC alias.
fn test_gc_array_fill_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [12];
$filled = array_fill(0, 2, $inner);
unset($inner);
echo $filled[0][0] . "|" . $filled[1][0];
"#,
    );
    assert_eq!(out, "12|12");
}

#[test]
// Verifies that an inner array used as array_combine value survives unset of the original.
// Fixture: create keys array and inner array as values, combine them, unset vals and inner, read from combined map.
// Regression: ensures array_combine value path preserves GC alias for nested arrays.
fn test_gc_array_combine_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [13];
$keys = ["keep"];
$vals = [$inner];
$map = array_combine($keys, $vals);
unset($vals);
unset($inner);
$saved = $map["keep"];
echo $saved[0];
"#,
    );
    assert_eq!(out, "13");
}

#[test]
// Verifies that an inner array used as array_fill_keys fill value survives unset of the original.
// Fixture: create keys ["a","b"] and inner array as fill value, call array_fill_keys, unset inner, read both map entries.
// Regression: ensures array_fill_keys fill-value path preserves GC alias for nested arrays.
fn test_gc_array_fill_keys_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [14];
$keys = ["a", "b"];
$map = array_fill_keys($keys, $inner);
unset($inner);
$first = $map["a"];
$second = $map["b"];
echo $first[0] . "|" . $second[0];
"#,
    );
    assert_eq!(out, "14|14");
}
