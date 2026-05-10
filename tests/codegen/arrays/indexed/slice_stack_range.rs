//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of indexed array array slicing, stack, and range builtins, including slice, shift, and shift empty.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_array_slice() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30, 40, 50];
$b = array_slice($a, 1, 3);
echo $b[0] . " " . $b[1] . " " . $b[2];
"#,
    );
    assert_eq!(out, "20 30 40");
}

#[test]
fn test_array_shift() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$first = array_shift($a);
echo $first . " " . count($a);
"#,
    );
    assert_eq!(out, "10 2");
}

#[test]
fn test_array_shift_empty() {
    let out = compile_and_run("<?php $a = [1]; array_shift($a); echo array_shift($a);");
    assert_eq!(out, "");
}

#[test]
fn test_array_unshift() {
    let out = compile_and_run(
        r#"<?php
$a = [2, 3];
$n = array_unshift($a, 1);
echo $n . " " . $a[0];
"#,
    );
    assert_eq!(out, "3 1");
}

#[test]
fn test_range() {
    let out = compile_and_run(
        r#"<?php
$a = range(1, 5);
echo count($a) . ":";
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "5:12345");
}

#[test]
fn test_range_descending() {
    let out = compile_and_run(
        r#"<?php
$a = range(5, 1);
echo count($a) . ":";
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "5:54321");
}

#[test]
fn test_range_single_element() {
    let out = compile_and_run(
        r#"<?php
$a = range(3, 3);
echo count($a) . ":";
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "1:3");
}
