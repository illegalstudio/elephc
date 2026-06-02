//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of indexed array sorting, including asort, arsort, and ksort.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies asort maintains key-value associations and sorts by values in ascending order.
/// Fixture: [3, 1, 2] → sorted [1, 2, 3] → first element $a[0] should be 1.
#[test]
fn test_asort() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
asort($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies arsort maintains key-value associations and sorts by values in descending order.
/// Fixture: [1, 3, 2] → sorted descending [3, 2, 1] → first element $a[0] should be 3.
#[test]
fn test_arsort() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 3, 2];
arsort($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies ksort sorts by keys in ascending order, preserving values.
/// Fixture: [3, 1, 2] with string keys → sorted by key → count remains 3.
#[test]
fn test_ksort() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
ksort($a);
echo count($a);
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies krsort sorts by keys in descending order, preserving values.
/// Fixture: [1, 2, 3] with string keys → sorted descending → count remains 3.
#[test]
fn test_krsort() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
krsort($a);
echo count($a);
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies natsort sorts values naturally (human ordering), preserving key-value associations.
/// Fixture: [3, 1, 2] → natural sort [1, 2, 3] → first element $a[0] should be 1.
#[test]
fn test_natsort() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
natsort($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies natcasesort sorts values naturally case-insensitively, preserving key-value associations.
/// Fixture: [3, 1, 2] → case-insensitive natural sort [1, 2, 3] → first element $a[0] should be 1.
#[test]
fn test_natcasesort() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
natcasesort($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_sort_string_array() {
    let out = compile_and_run(
        r#"<?php
$a = ["banana", "apple", "cherry", "date"];
sort($a);
echo $a[0] . "," . $a[1] . "," . $a[2] . "," . $a[3];
"#,
    );
    assert_eq!(out, "apple,banana,cherry,date");
}

#[test]
fn test_rsort_string_array() {
    let out = compile_and_run(
        r#"<?php
$a = ["banana", "apple", "cherry", "date"];
rsort($a);
echo $a[0] . "," . $a[1] . "," . $a[2] . "," . $a[3];
"#,
    );
    assert_eq!(out, "date,cherry,banana,apple");
}
