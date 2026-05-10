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
    let out = compile_and_run(
        r#"<?php
$result = [] + ["first", "second"];
echo count($result) . ":" . $result[0] . "," . $result[1];
"#,
    );
    assert_eq!(out, "2:first,second");
}
