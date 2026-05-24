//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of indexed array array search, merge, and union builtins, including search, search not found is strict false, and search assigned not found is strict false.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_array_search() {
    // Verifies `array_search` returns the 0-based integer index of the first match.
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_search(20, $a);
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_array_search_not_found_is_strict_false() {
    // Verifies `array_search` returns strict `false` (===) when the value is absent.
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_search(99, $a) === false ? "miss" : "hit";
"#,
    );
    assert_eq!(out, "miss");
}

#[test]
fn test_array_search_assigned_not_found_is_strict_false() {
    // Regression: assigning the result of `array_search` to a variable before comparing
    // must still yield strict `false`, not a falsy zero or empty string.
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$result = array_search(99, $a);
echo $result === false ? "miss" : "hit";
"#,
    );
    assert_eq!(out, "miss");
}

#[test]
fn test_array_search_zero_index_is_not_false() {
    // Verifies that `array_search` returns index `0` (not `false`) when the target is
    // at the first position, and that `=== false` correctly distinguishes the two.
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_search(10, $a) === false ? "miss" : "zero";
"#,
    );
    assert_eq!(out, "zero");
}

#[test]
fn test_array_key_exists() {
    // Verifies `array_key_exists` returns true for an existing integer key and false for a missing key.
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
if (array_key_exists(1, $a)) { echo "yes"; }
if (!array_key_exists(5, $a)) { echo "no"; }
"#,
    );
    assert_eq!(out, "yesno");
}

#[test]
fn test_array_merge() {
    // Verifies `array_merge` concatenates two indexed arrays and preserves all elements.
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

#[test]
fn test_indexed_array_union_keeps_left_duplicate_numeric_keys() {
    // Verifies the `+` operator keeps the left operand's values when both arrays
    // have the same numeric key (left wins semantics).
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

#[test]
fn test_indexed_array_union_string_values_append_missing_suffix() {
    // Verifies the `+` operator appends right-side string-keyed values that do not
    // exist in the left array (right-side keys are preserved for non-conflicting entries).
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

#[test]
fn test_indexed_array_union_empty_left_copies_right_layout() {
    // Verifies that an empty left array combined with `+` produces a result whose
    // indices and count mirror the right operand.
    let out = compile_and_run(
        r#"<?php
$result = [] + ["first", "second"];
echo count($result) . ":" . $result[0] . "," . $result[1];
"#,
    );
    assert_eq!(out, "2:first,second");
}
