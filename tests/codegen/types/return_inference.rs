//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types return type inference, including return type from foreach, return type mixed branches, and return type switch foreach.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

/// Verifies return type inference when a `foreach` carries a typed return out of a loop.
/// Fixture: `find()` uses `foreach` with an early `return "found"` and a fallback `return "not found"`.
/// Asserts that the returned string is correct when the target is found.
#[test]
fn test_return_type_from_foreach() {
    let out = compile_and_run(
        r#"<?php
function find($arr, $target) {
    foreach ($arr as $v) {
        if ($v === $target) { return "found"; }
    }
    return "not found";
}
echo find([1, 2, 3], 2);
"#,
    );
    assert_eq!(out, "found");
}

/// Verifies return type inference when branches return different types (`string` vs `int`).
/// The `describe()` function returns `"positive"` in the positive branch and `0` in the else branch.
/// Asserts that the branch that fires produces the correct output.
#[test]
fn test_return_type_mixed_branches() {
    let out = compile_and_run(
        r#"<?php
function describe($n) {
    if ($n > 0) { return "positive"; }
    return 0;
}
$r = describe(5);
echo $r;
"#,
    );
    assert_eq!(out, "positive");
}

/// Verifies return type inference when a `foreach` with a `switch` carries a typed return.
/// The `classify()` function returns `"zero"` or `"nonzero"` from inside a `switch` inside a `foreach`.
/// Asserts that the correct label is produced.
#[test]
fn test_return_type_switch_foreach() {
    let out = compile_and_run(
        r#"<?php
function classify($items) {
    foreach ($items as $item) {
        switch ($item) {
            case 0: return "zero";
            default: return "nonzero";
        }
    }
    return "empty";
}
echo classify([0]);
"#,
    );
    assert_eq!(out, "zero");
}

/// Verifies return type inference across an `if`/`else` with `string` returns in both branches.
/// The `check()` function returns `"big"` or `"small"` based on `$x > 10`.
/// Asserts both branches produce the correct concatenated output.
#[test]
fn test_return_string_from_else() {
    let out = compile_and_run(
        r#"<?php
function check($x) {
    if ($x > 10) {
        return "big";
    } else {
        return "small";
    }
}
echo check(5) . "|" . check(15);
"#,
    );
    assert_eq!(out, "small|big");
}

/// Verifies that a function with an `array` return type produces an array that is indexable.
/// The `getColor()` function returns `[255, 128, 0]` and the result is indexed with `$color[0]`.
/// Asserts that each array element is accessible and produces the correct values.
#[test]
fn test_array_return_type_survives_indexing() {
    let out = compile_and_run(
        r#"<?php
function getColor(): array {
    return [255, 128, 0];
}

$color = getColor();
echo $color[0] . "," . $color[1] . "," . $color[2];
"#,
    );
    assert_eq!(out, "255,128,0");
}

/// Verifies that `string` elements returned from a typed `array` parameter retain their `string` type
/// when passed to a function expecting `string`. The `pickSecond()` function takes an `array` and
/// passes `$names[1]` to `paint()` which expects `string`. Asserts that `bar` is echoed.
#[test]
fn test_string_array_element_keeps_string_type() {
    let out = compile_and_run(
        r#"<?php
function paint(string $name): string {
    return $name;
}

function pickSecond(array $names): string {
    return paint($names[1]);
}

echo pickSecond(["foo", "bar"]);
"#,
    );
    assert_eq!(out, "bar");
}

/// Verifies that `string` elements inside a `loadNames(): array` return value retain their type
/// when indexed and passed to a `string`-typed parameter. Asserts that `bar` is echoed.
#[test]
fn test_string_array_return_type_keeps_string_elements() {
    let out = compile_and_run(
        r#"<?php
function paint(string $name): string {
    return $name;
}

function loadNames(): array {
    return ["foo", "bar"];
}

$names = loadNames();
echo paint($names[1]);
"#,
    );
    assert_eq!(out, "bar");
}
