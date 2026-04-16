use crate::support::*;

// --- Global variables ---

#[test]
fn test_global_read() {
    let out = compile_and_run(
        r#"<?php
$x = 10;
function test() {
    global $x;
    echo $x;
}
test();
"#,
    );
    assert_eq!(out, "10");
}

#[test]
fn test_global_write() {
    let out = compile_and_run(
        r#"<?php
$y = 5;
function modify() {
    global $y;
    $y = 99;
}
modify();
echo $y;
"#,
    );
    assert_eq!(out, "99");
}

#[test]
fn test_global_read_write() {
    let out = compile_and_run(
        r#"<?php
$x = 10;
function test() {
    global $x;
    echo $x;
    $x = 20;
}
test();
echo $x;
"#,
    );
    assert_eq!(out, "1020");
}

#[test]
fn test_global_multiple_vars() {
    let out = compile_and_run(
        r#"<?php
$a = 1;
$b = 2;
function sum() {
    global $a, $b;
    echo $a + $b;
}
sum();
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_global_increment() {
    let out = compile_and_run(
        r#"<?php
$counter = 0;
function inc() {
    global $counter;
    $counter++;
}
inc();
inc();
inc();
echo $counter;
"#,
    );
    assert_eq!(out, "3");
}

// --- Static variables ---

#[test]
fn test_static_counter() {
    let out = compile_and_run(
        r#"<?php
function counter() {
    static $n = 0;
    $n++;
    echo $n;
}
counter();
counter();
counter();
"#,
    );
    assert_eq!(out, "123");
}

#[test]
fn test_static_preserves_value() {
    let out = compile_and_run(
        r#"<?php
function acc() {
    static $total = 0;
    $total = $total + 10;
    return $total;
}
echo acc();
echo acc();
echo acc();
"#,
    );
    assert_eq!(out, "102030");
}

#[test]
fn test_static_separate_functions() {
    let out = compile_and_run(
        r#"<?php
function a() {
    static $x = 0;
    $x++;
    echo $x;
}
function b() {
    static $x = 0;
    $x = $x + 10;
    echo $x;
}
a();
b();
a();
b();
"#,
    );
    assert_eq!(out, "110220");
}

// --- Pass by reference ---

#[test]
fn test_ref_increment() {
    let out = compile_and_run(
        r#"<?php
function increment(&$val) {
    $val++;
}
$x = 5;
increment($x);
echo $x;
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_ref_assign() {
    let out = compile_and_run(
        r#"<?php
function set_value(&$v, $new_val) {
    $v = $new_val;
}
$x = 1;
set_value($x, 42);
echo $x;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_ref_swap() {
    let out = compile_and_run(
        r#"<?php
function swap(&$a, &$b) {
    $tmp = $a;
    $a = $b;
    $b = $tmp;
}
$p = 1;
$q = 2;
swap($p, $q);
echo $p . $q;
"#,
    );
    assert_eq!(out, "21");
}

#[test]
fn test_ref_mixed_params() {
    let out = compile_and_run(
        r#"<?php
function add_to(&$target, $amount) {
    $target = $target + $amount;
}
$x = 10;
add_to($x, 5);
echo $x;
"#,
    );
    assert_eq!(out, "15");
}

// --- Variadic functions ---

#[test]
fn test_variadic_sum() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
echo sum(1, 2, 3);
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_variadic_five_args() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
echo sum(1, 2, 3, 4, 5);
"#,
    );
    assert_eq!(out, "15");
}

#[test]
fn test_variadic_multiple_calls_same_function() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
echo sum(1, 2, 3);
echo ":";
echo sum(10, 20, 30, 40, 50);
"#,
    );
    assert_eq!(out, "6:150");
}

#[test]
fn test_variadic_empty() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
echo sum();
"#,
    );
    assert_eq!(out, "0");
}

#[test]
fn test_variadic_with_regular_params() {
    let out = compile_and_run(
        r#"<?php
function greet($greeting, ...$names) {
    foreach ($names as $name) {
        echo $greeting . " " . $name . "\n";
    }
}
greet("Hello", "Alice", "Bob");
"#,
    );
    assert_eq!(out, "Hello Alice\nHello Bob\n");
}

#[test]
fn test_variadic_count() {
    let out = compile_and_run(
        r#"<?php
function num_args(...$args) {
    return count($args);
}
echo num_args(10, 20, 30, 40);
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_variadic_single_arg() {
    let out = compile_and_run(
        r#"<?php
function wrap(...$items) {
    return $items;
}
$arr = wrap(42);
echo $arr[0];
"#,
    );
    assert_eq!(out, "42");
}

// --- Spread operator ---

#[test]
fn test_spread_in_function_call() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
$args = [10, 20, 30];
echo sum(...$args);
"#,
    );
    assert_eq!(out, "60");
}

#[test]
fn test_spread_in_array_literal() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = [3, 4];
$c = [...$a, ...$b];
echo count($c);
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_spread_array_values() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = [3, 4];
$c = [...$a, ...$b];
foreach ($c as $v) {
    echo $v;
}
"#,
    );
    assert_eq!(out, "1234");
}

#[test]
fn test_spread_mixed_with_elements() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = [5, 6];
$c = [...$a, 3, 4, ...$b];
echo count($c);
echo " ";
foreach ($c as $v) {
    echo $v;
}
"#,
    );
    assert_eq!(out, "6 123456");
}

#[test]
fn test_spread_single_source() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
$c = [...$a];
echo count($c);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_variadic_with_regular_and_no_extra() {
    let out = compile_and_run(
        r#"<?php
function prefix($pre, ...$items) {
    echo count($items);
}
prefix("x");
"#,
    );
    assert_eq!(out, "0");
}

