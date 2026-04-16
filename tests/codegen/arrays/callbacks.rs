use crate::support::*;

// --- Callback-based array functions ---

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

#[test]
fn test_function_exists_false() {
    let out = compile_and_run(
        r#"<?php
if (function_exists("nonexistent")) { echo "yes"; } else { echo "no"; }
"#,
    );
    assert_eq!(out, "no");
}

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

