//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of indexed array array search, merge, and union builtins, including search, search not found is strict false, and search assigned not found is strict false.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies `array_search` returns the 0-based integer index of the first match.
#[test]
fn test_array_search() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_search(20, $a);
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies `array_search` returns strict `false` (===) when the value is absent.
#[test]
fn test_array_search_not_found_is_strict_false() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_search(99, $a) === false ? "miss" : "hit";
"#,
    );
    assert_eq!(out, "miss");
}

/// Regression: assigning the result of `array_search` to a variable before comparing
/// must still yield strict `false`, not a falsy zero or empty string.
#[test]
fn test_array_search_assigned_not_found_is_strict_false() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$result = array_search(99, $a);
echo $result === false ? "miss" : "hit";
"#,
    );
    assert_eq!(out, "miss");
}

/// Verifies that `array_search` returns index `0` (not `false`) when the target is
/// at the first position, and that `=== false` correctly distinguishes the two.
#[test]
fn test_array_search_zero_index_is_not_false() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_search(10, $a) === false ? "miss" : "zero";
"#,
    );
    assert_eq!(out, "zero");
}

/// Verifies `array_key_exists` returns true for an existing integer key and false for a missing key.
#[test]
fn test_array_key_exists() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
if (array_key_exists(1, $a)) { echo "yes"; }
if (!array_key_exists(5, $a)) { echo "no"; }
"#,
    );
    assert_eq!(out, "yesno");
}

/// Verifies `array_merge` concatenates two indexed arrays and preserves all elements.
#[test]
fn test_array_merge() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = [3, 4];
$c = array_merge($a, $b);
echo count($c);
echo $c[0] . $c[1] . $c[2] . $c[3];
"#,
    );
    assert_eq!(out, "41234");
}

/// Verifies `array_merge` over two indexed STRING arrays keeps every element intact.
///
/// String arrays use 16-byte (ptr+len) slots; the dedicated `__rt_array_merge_str` helper must copy
/// each slot, or every element past the first would be corrupted by the 8-byte scalar merge helper.
#[test]
fn test_array_merge_string_arrays() {
    let out = compile_and_run(
        r#"<?php
$a = ["apple", "banana"];
$b = ["cherry", "date"];
$c = array_merge($a, $b);
echo count($c);
echo ":";
echo $c[0]; echo ","; echo $c[1]; echo ","; echo $c[2]; echo ","; echo $c[3];
"#,
    );
    assert_eq!(out, "4:apple,banana,cherry,date");
}

/// Verifies `array_merge([], $strings)` adopts the string element type (16-byte slot path) so the
/// common `$r = []; array_merge($r, $strs)` accumulation shape returns intact strings.
#[test]
fn test_array_merge_empty_left_with_string_right() {
    let out = compile_and_run(
        r#"<?php
$r = [];
$s = ["cherry", "date", "fig"];
$m = array_merge($r, $s);
echo count($m);
echo ":";
echo $m[0]; echo ","; echo $m[1]; echo ","; echo $m[2];
"#,
    );
    assert_eq!(out, "3:cherry,date,fig");
}

/// Verifies `array_merge` uses the right operand element type when the left array is empty.
#[test]
fn test_array_merge_empty_left_uses_right_element_type() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$b = [3, 4];
$c = array_merge($a, $b);
echo count($c);
echo ":";
echo $c[0] . $c[1];
"#,
    );
    assert_eq!(out, "2:34");
}

/// Verifies the `+` operator keeps the left operand's values when both arrays
/// have the same numeric key (left wins semantics).
#[test]
fn test_indexed_array_union_keeps_left_duplicate_numeric_keys() {
    let out = compile_and_run(
        r#"<?php
$left = [10, 20];
$right = [99, 88, 77];
$result = $left + $right;
echo count($result) . ":" . $result[0] . "," . $result[1] . "," . $result[2];
"#,
    );
    assert_eq!(out, "3:10,20,77");
}

/// Verifies the `+` operator appends right-side string-keyed values that do not
/// exist in the left array (right-side keys are preserved for non-conflicting entries).
#[test]
fn test_indexed_array_union_string_values_append_missing_suffix() {
    let out = compile_and_run(
        r#"<?php
$left = ["left"];
$right = ["ignored", "added"];
$result = $left + $right;
echo count($result) . ":" . $result[0] . "," . $result[1];
"#,
    );
    assert_eq!(out, "2:left,added");
}

/// Verifies that an empty left array combined with `+` produces a result whose
/// indices and count mirror the right operand.
#[test]
fn test_indexed_array_union_empty_left_copies_right_layout() {
    let out = compile_and_run(
        r#"<?php
$result = [] + ["first", "second"];
echo count($result) . ":" . $result[0] . "," . $result[1];
"#,
    );
    assert_eq!(out, "2:first,second");
}

/// Verifies in_array()/array_search() accept the optional `$strict` flag instead of failing to
/// compile. elephc's element comparison is value/byte-exact, so it already yields strict semantics
/// for a needle whose type matches the array element type (the common case). Uses string and
/// integer arrays for in_array and an integer array for array_search (array_search on
/// string-element arrays is a separate pre-existing gap).
#[test]
fn test_in_array_and_array_search_accept_strict_flag() {
    let out = compile_and_run(
        r#"<?php
$nums = [1, 2, 3];
$strs = ["a", "b", "c"];
$vals = [10, 20, 30];
echo in_array(3, $nums, true) ? "1" : "0";
echo in_array(9, $nums, true) ? "1" : "0";
echo in_array("b", $strs, true) ? "1" : "0";
echo "|";
echo array_search(20, $vals, true);
echo array_search(99, $vals, true) === false ? "F" : "?";
"#,
    );
    assert_eq!(out, "101|1F");
}

/// Verifies a non-literal `$strict` argument is still evaluated for its side effects, even though
/// elephc's same-type comparison does not depend on the flag value. The side-effecting call must
/// run exactly once before the search result is produced.
#[test]
fn test_in_array_strict_argument_side_effect_evaluated() {
    let out = compile_and_run(
        r#"<?php
function strictFlag() { echo "S"; return true; }
$r = in_array(2, [1, 2, 3], strictFlag());
echo $r ? "Y" : "N";
"#,
    );
    assert_eq!(out, "SY");
}

/// Verifies `array_merge` accepts three or more arrays (variadic), concatenating them left-to-right.
/// Lowered by folding into left-nested two-array calls; covers int and string element arrays and the
/// empty-left accumulator adopting a later operand's element type.
#[test]
fn test_array_merge_three_or_more_arrays() {
    let out = compile_and_run(
        r#"<?php
$ints = array_merge([1, 2], [3], [4, 5], [6]);
echo count($ints); echo ":"; echo implode(",", $ints); echo "|";
$strs = array_merge(["a"], ["b", "c"], ["d"]);
echo implode(",", $strs); echo "|";
$acc = array_merge([], [7], [8, 9]);
echo implode(",", $acc);
"#,
    );
    assert_eq!(out, "6:1,2,3,4,5,6|a,b,c,d|7,8,9");
}
