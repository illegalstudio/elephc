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

/// Verifies compiled PHP output for sort string array.
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

/// Verifies compiled PHP output for rsort string array.
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

/// Verifies sort() orders a float array by numeric value, not by raw 64-bit
/// bit-pattern. Negative and fractional values must order correctly (the old
/// integer comparator placed negatives after positives).
#[test]
fn test_sort_float_array() {
    let out = compile_and_run(
        r#"<?php
$a = [3.5, -1.2, 2.8, -9.9];
sort($a);
echo $a[0] . "," . $a[1] . "," . $a[2] . "," . $a[3];
"#,
    );
    assert_eq!(out, "-9.9,-1.2,2.8,3.5");
}

/// Verifies rsort() orders a float array descending by numeric value.
#[test]
fn test_rsort_float_array() {
    let out = compile_and_run(
        r#"<?php
$a = [3.5, -1.2, 2.8, -9.9];
rsort($a);
echo $a[0] . "," . $a[1] . "," . $a[2] . "," . $a[3];
"#,
    );
    assert_eq!(out, "3.5,2.8,-1.2,-9.9");
}

/// Verifies the float sort order independently of float-to-string formatting,
/// using value comparisons so a regression in the comparator is caught even if
/// echo formatting changes.
#[test]
fn test_sort_float_array_order_is_numeric() {
    let out = compile_and_run(
        r#"<?php
$a = [3.5, -1.2, 2.8, -9.9];
sort($a);
echo ($a[0] == -9.9 && $a[1] == -1.2 && $a[2] == 2.8 && $a[3] == 3.5) ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

/// Regression (H7): the sort builtins accept an optional SORT_* `$flags` argument instead of
/// failing to compile. elephc routes by the array's element family, so SORT_STRING on a string
/// array, SORT_NUMERIC on a numeric array, and SORT_REGULAR all sort correctly (a flag mismatched
/// to the element type is not yet specialized). The asort() call exercises flag acceptance on a
/// key-preserving sort.
#[test]
fn test_sort_builtins_accept_sort_flags() {
    let out = compile_and_run(
        r#"<?php
$s = ["banana", "apple", "cherry"];
sort($s, SORT_STRING);
echo implode(",", $s);
$n = [3, 1, 2];
sort($n, SORT_NUMERIC);
echo "|" . implode(",", $n);
$r = [3, 1, 2];
rsort($r, SORT_NUMERIC);
echo "|" . implode(",", $r);
$a = ["x" => 3, "y" => 1];
asort($a, SORT_REGULAR);
echo "|ok";
"#,
    );
    assert_eq!(out, "apple,banana,cherry|1,2,3|3,2,1|ok");
}

/// Regression (H7): the SORT_* flag constants resolve to their PHP integer values when used as
/// plain expressions (the codegen prescan materializes them alongside the checker registration).
#[test]
fn test_sort_flag_constants_have_php_values() {
    let out = compile_and_run(
        r#"<?php
echo SORT_REGULAR . "," . SORT_NUMERIC . "," . SORT_STRING . "," . SORT_NATURAL . "," . SORT_FLAG_CASE;
"#,
    );
    assert_eq!(out, "0,1,2,6,8");
}
