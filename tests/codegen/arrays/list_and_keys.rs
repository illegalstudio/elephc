//! Purpose:
//! Integration tests for the `array_is_list`, `array_key_first`, and `array_key_last` builtins.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries; assertions compare stdout.
//! - Covers indexed arrays (compile-time list shape), associative hashes (runtime walk),
//!   integer/string hash keys, empty containers (null key), and case-insensitive calls.

use crate::support::*;

// --- array_is_list ---

/// Verifies array_is_list() is true for an indexed array and false for string-keyed
/// and offset-keyed associative arrays.
/// Fixture: a packed indexed array, a string-keyed hash, and a hash whose keys start at 5.
#[test]
fn test_array_is_list_basic() {
    let out = compile_and_run(
        r#"<?php
echo array_is_list([1, 2, 3]) ? "y" : "n";
echo array_is_list(["a" => 1, "b" => 2]) ? "y" : "n";
echo array_is_list([5 => "x", 6 => "y"]) ? "y" : "n";
echo array_is_list([]) ? "y" : "n";
"#,
    );
    assert_eq!(out, "ynny");
}

/// Verifies array_is_list() walks a hash produced by json_decode($s, true): a JSON array
/// decodes to a list-shaped hash (true), a JSON object decodes to a string-keyed hash (false).
/// Fixture: json_decode of a numeric array and of an object, both as associative.
#[test]
fn test_array_is_list_runtime_hash() {
    let out = compile_and_run(
        r#"<?php
$a = json_decode('[10, 20, 30]', true);
echo array_is_list($a) ? "y" : "n";
$b = json_decode('{"x": 1, "y": 2}', true);
echo array_is_list($b) ? "y" : "n";
"#,
    );
    assert_eq!(out, "yn");
}

/// Verifies array_is_list() is callable case-insensitively, matching PHP builtin name rules.
/// Fixture: mixed-case spelling of the builtin over a packed indexed array.
#[test]
fn test_array_is_list_case_insensitive() {
    let out = compile_and_run(r#"<?php echo Array_Is_List([1, 2, 3]) ? "y" : "n";"#);
    assert_eq!(out, "y");
}

// --- array_key_first / array_key_last ---

/// Verifies array_key_first()/array_key_last() return positional integer keys for indexed arrays.
/// Fixture: a three-element indexed array; first key 0, last key 2.
#[test]
fn test_array_key_edge_indexed() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_key_first($a);
echo array_key_last($a);
"#,
    );
    assert_eq!(out, "02");
}

/// Verifies array_key_first()/array_key_last() return string keys in insertion order.
/// Fixture: a string-keyed associative array with three entries.
#[test]
fn test_array_key_edge_assoc_string() {
    let out = compile_and_run(
        r#"<?php
$m = ["x" => 1, "y" => 2, "z" => 3];
echo array_key_first($m);
echo array_key_last($m);
"#,
    );
    assert_eq!(out, "xz");
}

/// Verifies array_key_first()/array_key_last() return integer keys from an out-of-order hash.
/// Fixture: an integer-keyed associative array inserted as 3, 1, 7; first 3, last 7.
#[test]
fn test_array_key_edge_assoc_int() {
    let out = compile_and_run(
        r#"<?php
$m = [3 => "a", 1 => "b", 7 => "c"];
echo array_key_first($m);
echo array_key_last($m);
"#,
    );
    assert_eq!(out, "37");
}

/// Verifies array_key_first()/array_key_last() return null for an empty array.
/// Fixture: an empty array literal compared strictly against null.
#[test]
fn test_array_key_edge_empty_is_null() {
    let out = compile_and_run(
        r#"<?php
echo (array_key_first([]) === null) ? "first-null" : "first-val";
echo (array_key_last([]) === null) ? "-last-null" : "-last-val";
"#,
    );
    assert_eq!(out, "first-null-last-null");
}
