//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of callables state and variadics, including global read, global write, and global read write.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

// --- Global variables ---

/// Verifies that a `global $var` declaration inside a function reads the correct global value.
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

/// Verifies that a `global $var` declaration inside a function can write to a global variable.
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

/// Verifies that a `global $var` declaration allows both reading and writing the global variable.
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

/// Verifies that multiple comma-separated global variables can be declared in one statement.
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

/// Verifies that global variables persist and are correctly mutated across multiple function calls.
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

/// Verifies that a static variable inside a function increments across multiple invocations.
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

/// Verifies that a static variable inside a closure links and persists across calls.
#[test]
fn test_closure_static_local_preserves_value_across_calls() {
    let out = compile_and_run(
        r#"<?php
$f = function () {
    static $x = 0;
    echo ++$x;
};
$f();
$f();
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies that a static variable's value is preserved and updated correctly across calls.
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

/// Verifies that two functions can each declare a static variable with the same name without interference.
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

/// Verifies that a `&$var` parameter increments the caller's variable in place.
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

/// Verifies that a `&$var` parameter can be assigned a new value and the caller sees the change.
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

/// Verifies direct reference assignment aliases reads from the source variable.
#[test]
fn test_reference_assignment_alias_reads_source() {
    let out = compile_and_run(
        r#"<?php
$a = 1;
$b =& $a;
echo $b;
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies writes through a directly aliased variable update the original source.
#[test]
fn test_reference_assignment_alias_writes_through() {
    let out = compile_and_run(
        r#"<?php
$a = 1;
$b =& $a;
$b = 42;
echo $a;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies writes to the original source remain visible through the alias.
#[test]
fn test_reference_assignment_source_write_visible_through_alias() {
    let out = compile_and_run(
        r#"<?php
$a = 1;
$b =& $a;
$a = 2;
echo $b;
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies that a two-argument `&$a, &$b` swap function correctly swaps the caller's values.
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

/// Verifies that a `&$target` parameter with a regular by-value parameter works correctly.
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

/// Verifies a variadic function collects exactly three positional arguments into the rest array.
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

/// Verifies a variadic function collects exactly five positional arguments into the rest array.
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

/// Verifies that a variadic function can be called multiple times with different argument counts without interference.
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

/// Verifies that a variadic function called with no arguments receives an empty rest array.
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

/// Verifies that a variadic parameter follows regular positional parameters and collects remaining arguments.
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

/// Verifies that `count()` works correctly on a variadic rest array with four elements.
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

/// Verifies that a variadic function returning the rest array allows accessing the single wrapped element.
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

/// Verifies that a nested array passed to a variadic function preserves its element tag through json_encode.
#[test]
fn test_variadic_array_arg_preserves_runtime_element_tag() {
    let out = compile_and_run(
        r#"<?php
function wrap(...$items) {
    echo json_encode($items);
}
wrap([1, 2]);
"#,
    );
    assert_eq!(out, "[[1,2]]");
}

// --- Spread operator ---

/// Verifies that an array spread `...$args` in a function call unpacks correctly into a variadic callee.
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

/// Verifies that an array spread into a function with regular and variadic params fills regular params first and collects the remainder into the rest array.
#[test]
fn test_spread_in_variadic_function_fills_regular_params_first() {
    let out = compile_and_run(
        r#"<?php
function show($head, ...$rest) {
    echo "head=" . $head . ";";
    foreach ($rest as $value) {
        echo $value . ";";
    }
}
show(...[1, 2, 3]);
"#,
    );
    assert_eq!(out, "head=1;2;3;");
}

/// Verifies that two spread arrays in an array literal `[...$a, ...$b]` produce a flattened array of four elements.
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

/// Verifies that two spread arrays in an array literal produce a flattened array whose elements iterate in correct order.
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

/// Verifies that array spreads can be interleaved with literal elements in an array literal.
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

/// Verifies that a single-array spread `[...$a]` produces an array equal in length to the source.
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

/// Verifies that a variadic function with a preceding regular parameter receives zero rest elements when called with exactly one argument.
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
