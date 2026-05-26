//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of array array callbacks, including map, map single, and map string values to ints.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

// --- Callback-based array functions ---

// Tests `array_map` with a user-defined callback that doubles each element.
/// Verifies that array map.
#[test]
fn test_array_map() {
    let out = compile_and_run(
        r#"<?php
function double($x) { return $x * 2; }
$a = [1, 2, 3];
$b = array_map("double", $a);
echo $b[0] . $b[1] . $b[2];
"#,
    );
    assert_eq!(out, "246");
}

// Tests `array_map` on a single-element array with a user-defined increment callback.
/// Verifies that array map single.
#[test]
fn test_array_map_single() {
    let out = compile_and_run(
        r#"<?php
function inc($x) { return $x + 1; }
$a = [10];
$b = array_map("inc", $a);
echo $b[0];
"#,
    );
    assert_eq!(out, "11");
}

// Tests `array_map` with a typed builtin callback (`strlen`) applied to string values,
// verifying mixed-type result handling in array_map codegen.
/// Verifies that array map string values to ints.
#[test]
fn test_array_map_string_values_to_ints() {
    let out = compile_and_run(
        r#"<?php
function string_len(string $value) { return strlen($value); }
$a = ["aa", "bbbb"];
$b = array_map("string_len", $a);
echo $b[0];
echo ",";
echo $b[1];
"#,
    );
    assert_eq!(out, "2,4");
}

// Tests `array_filter` with a predicate that keeps even integers, verifying correct
// iteration, element removal, and count/foreach output.
/// Verifies that array filter.
#[test]
fn test_array_filter() {
    let out = compile_and_run(
        r#"<?php
function is_even($x) { return $x % 2 == 0; }
$a = [1, 2, 3, 4, 5, 6];
$b = array_filter($a, "is_even");
echo count($b);
foreach ($b as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "3246");
}

// Tests `array_filter` with a typed builtin callback (`str_starts_with`) applied to
// string values, verifying correct filtering and mixed string output.
/// Verifies that array filter string values.
#[test]
fn test_array_filter_string_values() {
    let out = compile_and_run(
        r#"<?php
function starts_a(string $value) { return str_starts_with($value, "a"); }
$a = ["aa", "bb", "ab"];
$b = array_filter($a, "starts_a");
echo count($b);
foreach ($b as $value) { echo $value; }
"#,
    );
    assert_eq!(out, "2aaab");
}

// Tests `array_filter` when the callback returns falsy for every element, producing
// an empty array and confirming count is 0.
/// Verifies that array filter none pass.
#[test]
fn test_array_filter_none_pass() {
    let out = compile_and_run(
        r#"<?php
function never($x) { return 0; }
$a = [1, 2, 3];
$b = array_filter($a, "never");
echo count($b);
"#,
    );
    assert_eq!(out, "0");
}

// Tests `array_reduce` with a two-argument user callback (carry + item) over a five-element
// array, providing an explicit initial value of 0.
/// Verifies that array reduce.
#[test]
fn test_array_reduce() {
    let out = compile_and_run(
        r#"<?php
function add($carry, $item) { return $carry + $item; }
$a = [1, 2, 3, 4, 5];
$sum = array_reduce($a, "add", 0);
echo $sum;
"#,
    );
    assert_eq!(out, "15");
}

// Tests `array_reduce` with a user callback (carry * item) and an explicit initial
// value of 1, verifying the carry accumulates correctly across the array.
/// Verifies that array reduce with initial.
#[test]
fn test_array_reduce_with_initial() {
    let out = compile_and_run(
        r#"<?php
function mul($carry, $item) { return $carry * $item; }
$a = [2, 3, 4];
$product = array_reduce($a, "mul", 1);
echo $product;
"#,
    );
    assert_eq!(out, "24");
}

// Tests `array_walk` with a callback that echoes each element, verifying the function
// walks by reference and mutates the array in place.
/// Verifies that array walk.
#[test]
fn test_array_walk() {
    let out = compile_and_run(
        r#"<?php
function show($x) { echo $x; }
$a = [10, 20, 30];
array_walk($a, "show");
"#,
    );
    assert_eq!(out, "102030");
}

// Tests `usort` with a comparison callback that sorts an unsorted array in ascending
// order, verifying both value ordering and that the array is modified in place.
/// Verifies that usort.
#[test]
fn test_usort() {
    let out = compile_and_run(
        r#"<?php
function cmp($a, $b) { return $a - $b; }
$a = [5, 3, 1, 4, 2];
usort($a, "cmp");
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "12345");
}

// Tests `usort` with a comparison callback that reverses order (`b - a`), verifying
// values are re-sorted in descending order.
/// Verifies that usort reverse.
#[test]
fn test_usort_reverse() {
    let out = compile_and_run(
        r#"<?php
function rcmp($a, $b) { return $b - $a; }
$a = [1, 3, 2];
usort($a, "rcmp");
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "321");
}

// Tests `uksort` with a comparison callback, verifying keys are reordered while values
// stay associated with their original keys.
/// Verifies that uksort.
#[test]
fn test_uksort() {
    let out = compile_and_run(
        r#"<?php
function cmp($a, $b) { return $a - $b; }
$a = [5, 3, 1, 4, 2];
uksort($a, "cmp");
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "12345");
}

// Tests `uasort` with a comparison callback, verifying values are sorted while preserving
// key-to-value associations.
/// Verifies that uasort.
#[test]
fn test_uasort() {
    let out = compile_and_run(
        r#"<?php
function cmp($a, $b) { return $a - $b; }
$a = [30, 10, 20];
uasort($a, "cmp");
foreach ($a as $value) { echo $value . " "; }
"#,
    );
    assert_eq!(out, "10 20 30 ");
}

// Tests `call_user_func` with a user-defined function and a single argument.
/// Verifies that call user func.
#[test]
fn test_call_user_func() {
    let out = compile_and_run(
        r#"<?php
function greet($x) { return $x + 100; }
$result = call_user_func("greet", 42);
echo $result;
"#,
    );
    assert_eq!(out, "142");
}

// Tests `call_user_func` with a user-defined function that takes no arguments.
/// Verifies that call user func no args.
#[test]
fn test_call_user_func_no_args() {
    let out = compile_and_run(
        r#"<?php
function get_value() { return 99; }
$result = call_user_func("get_value");
echo $result;
"#,
    );
    assert_eq!(out, "99");
}

// Tests `call_user_func` with a function accepting 9 parameters and 9 overflow arguments
// passed on the stack, verifying stack-passed overflow argument handling.
/// Verifies that call user func supports stack passed overflow args.
#[test]
fn test_call_user_func_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
function sum9($a, $b, $c, $d, $e, $f, $g, $h, $i) {
    return $a + $b + $c + $d + $e + $f + $g + $h + $i;
}
echo call_user_func("sum9", 1, 2, 3, 4, 5, 6, 7, 8, 9);
"#,
    );
    assert_eq!(out, "45");
}

// Tests `call_user_func` with a builtin function name (`STRLEN`) passed as a string,
// verifying case-insensitive builtin callback resolution.
/// Verifies that call user func string builtin callback.
#[test]
fn test_call_user_func_string_builtin_callback() {
    let out = compile_and_run(r#"<?php echo call_user_func("STRLEN", "hello");"#);
    assert_eq!(out, "5");
}

// Tests `function_exists` returns true for a user-defined function.
/// Verifies that call user func accepts callable without known signature.
#[test]
fn test_call_user_func_accepts_callable_without_known_signature() {
    let out = compile_and_run(
        r#"<?php
function make_callback(): callable {
    return function($a, $b, $c): int {
        return $a + $b + $c;
    };
}
echo call_user_func(make_callback(), 2, 3, 4);
"#,
    );
    assert_eq!(out, "9");
}

/// Verifies that function exists true.
#[test]
fn test_function_exists_true() {
    let out = compile_and_run(
        r#"<?php
function my_func() { return 1; }
if (function_exists("my_func")) { echo "yes"; } else { echo "no"; }
"#,
    );
    assert_eq!(out, "yes");
}

// Tests `function_exists` returns false for a non-existent function name.
/// Verifies that function exists false.
#[test]
fn test_function_exists_false() {
    let out = compile_and_run(
        r#"<?php
if (function_exists("nonexistent")) { echo "yes"; } else { echo "no"; }
"#,
    );
    assert_eq!(out, "no");
}

// Tests `usort` on an already-sorted array, verifying the comparator is still called and
// the output is unchanged (regression: no sorting skipped incorrectly).
/// Verifies that usort already sorted.
#[test]
fn test_usort_already_sorted() {
    let out = compile_and_run(
        r#"<?php
function cmp($a, $b) { return $a - $b; }
$a = [1, 2, 3];
usort($a, "cmp");
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "123");
}

// Tests `usort` on a single-element array, verifying the comparator is called and the
// array remains unchanged.
/// Verifies that usort single element.
#[test]
fn test_usort_single_element() {
    let out = compile_and_run(
        r#"<?php
function cmp($a, $b) { return $a - $b; }
$a = [42];
usort($a, "cmp");
echo $a[0];
"#,
    );
    assert_eq!(out, "42");
}

// Tests `array_map` with a callback that squares each element, verifying correct
// element mapping on a four-element array.
/// Verifies that array map with complex callback.
#[test]
fn test_array_map_with_complex_callback() {
    let out = compile_and_run(
        r#"<?php
function square($x) { return $x * $x; }
$a = [1, 2, 3, 4];
$b = array_map("square", $a);
echo $b[0] . " " . $b[1] . " " . $b[2] . " " . $b[3];
"#,
    );
    assert_eq!(out, "1 4 9 16");
}

// Tests `array_reduce` on a single-element array with an initial carry value of 100,
// verifying the callback is invoked once with the carry and item.
/// Verifies that array reduce single.
#[test]
fn test_array_reduce_single() {
    let out = compile_and_run(
        r#"<?php
function add($carry, $item) { return $carry + $item; }
$a = [42];
$sum = array_reduce($a, "add", 100);
echo $sum;
"#,
    );
    assert_eq!(out, "142");
}
