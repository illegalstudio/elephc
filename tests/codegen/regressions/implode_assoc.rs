//! Purpose:
//! Regression tests for `implode()` over associative (string-keyed) arrays.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout.

use super::*;

/// Tests implode over an associative array of string values.
#[test]
fn test_implode_assoc_string_values() {
    let out = compile_and_run(
        r#"<?php
$m = ['a'=>'x','b'=>'y','c'=>'z'];
echo implode('-', $m);
"#,
    );
    assert_eq!(out, "x-y-z");
}

/// Tests implode over an associative array of integer values uses the int helper.
#[test]
fn test_implode_assoc_int_values() {
    let out = compile_and_run(
        r#"<?php
$m = ['a'=>1,'b'=>2,'c'=>3];
echo implode(',', $m);
"#,
    );
    assert_eq!(out, "1,2,3");
}

/// Tests implode over a superglobal holding mixed string values.
#[test]
fn test_implode_assoc_superglobal_mixed() {
    let out = compile_and_run(
        r#"<?php
$_GET = ['a'=>'1','b'=>'2'];
echo implode('|', $_GET);
"#,
    );
    assert_eq!(out, "1|2");
}

/// Tests implode on an associative array after removing its only entry.
#[test]
fn test_implode_assoc_empty() {
    let out = compile_and_run(
        r#"<?php
$m = ['a'=>'x'];
unset($m['a']);
echo '[' . implode(',', $m) . ']';
"#,
    );
    assert_eq!(out, "[]");
}

/// Tests implode on a single-entry associative array.
#[test]
fn test_implode_assoc_single() {
    let out = compile_and_run(
        r#"<?php
$m = ['only'=>'v'];
echo implode(',', $m);
"#,
    );
    assert_eq!(out, "v");
}

/// Tests that insertion order is preserved after later writes to an associative array.
#[test]
fn test_implode_assoc_insertion_order() {
    let out = compile_and_run(
        r#"<?php
$m = ['z'=>'1'];
$m['a'] = '2';
echo implode(',', $m);
"#,
    );
    assert_eq!(out, "1,2");
}

/// Tests repeated implode over the same associative array to exercise temp-array cleanup.
#[test]
fn test_implode_assoc_loop_cleanup() {
    let out = compile_and_run(
        r#"<?php
$m = ['a'=>'x','b'=>'y'];
for ($i = 0; $i < 3; $i++) {
    echo implode('-', $m);
    if ($i < 2) echo '|';
}
"#,
    );
    assert_eq!(out, "x-y|x-y|x-y");
}
