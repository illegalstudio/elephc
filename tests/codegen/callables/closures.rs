//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of callables closures, including closure basic, closure multiple params, and arrow function basic.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

// --- Anonymous functions (closures) and arrow functions ---

#[test]
fn test_closure_basic() {
    let out = compile_and_run(
        r#"<?php
$double = function($x) { return $x * 2; };
echo $double(5);
"#,
    );
    assert_eq!(out, "10");
}

#[test]
fn test_closure_multiple_params() {
    let out = compile_and_run(
        r#"<?php
$add = function($a, $b) { return $a + $b; };
echo $add(3, 7);
"#,
    );
    assert_eq!(out, "10");
}

#[test]
fn test_arrow_function_basic() {
    let out = compile_and_run(
        r#"<?php
$triple = fn($x) => $x * 3;
echo $triple(4);
"#,
    );
    assert_eq!(out, "12");
}

#[test]
fn test_arrow_function_expression() {
    let out = compile_and_run(
        r#"<?php
$calc = fn($x) => $x * $x + 1;
echo $calc(5);
"#,
    );
    assert_eq!(out, "26");
}

#[test]
fn test_closure_return_type_annotation() {
    let out = compile_and_run(
        r#"<?php
$prefix = "id:";
$format = function(int $value) use ($prefix): string {
    return $prefix . $value;
};
echo $format(7);
"#,
    );
    assert_eq!(out, "id:7");
}

#[test]
fn test_closure_return_type_annotation_uses_typed_param() {
    let out = compile_and_run(
        r#"<?php
$identity = function(string $value): string {
    return $value;
};
echo $identity("ok");
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_arrow_return_type_annotation() {
    let out = compile_and_run(
        r#"<?php
$double = fn(int $value): int => $value * 2;
echo $double(9);
"#,
    );
    assert_eq!(out, "18");
}

#[test]
fn test_iife_arrow_return_type_annotation() {
    let out = compile_and_run(
        r#"<?php
echo (fn(): string => "ready")();
"#,
    );
    assert_eq!(out, "ready");
}

#[test]
fn test_closure_array_map() {
    let out = compile_and_run(
        r#"<?php
$result = array_map(function($x) { return $x * 10; }, [1, 2, 3]);
echo $result[0];
echo $result[1];
echo $result[2];
"#,
    );
    assert_eq!(out, "102030");
}

#[test]
fn test_arrow_function_array_map() {
    let out = compile_and_run(
        r#"<?php
$result = array_map(fn(int $x): int => $x + 100, [1, 2, 3]);
echo $result[0];
echo $result[1];
echo $result[2];
"#,
    );
    assert_eq!(out, "101102103");
}

#[test]
fn test_captured_closure_array_map() {
    let out = compile_and_run(
        r#"<?php
$factor = 7;
$result = array_map(function($x) use ($factor) { return $x * $factor; }, [1, 2, 3]);
echo $result[0];
echo $result[1];
echo $result[2];
"#,
    );
    assert_eq!(out, "71421");
}

#[test]
fn test_captured_closure_variable_array_map() {
    let out = compile_and_run(
        r#"<?php
$offset = 5;
$add = function($x) use ($offset) { return $x + $offset; };
$result = array_map($add, [10, 20]);
echo $result[0];
echo $result[1];
"#,
    );
    assert_eq!(out, "1525");
}

#[test]
fn test_captured_closure_variable_array_map_string_capture() {
    let out = compile_and_run(
        r#"<?php
$prefix = "id:";
$format = function(int $value) use ($prefix): string {
    return $prefix . $value;
};
$result = array_map($format, [7, 8]);
echo $result[0];
echo ",";
echo $result[1];
"#,
    );
    assert_eq!(out, "id:7,id:8");
}

#[test]
fn test_captured_closure_variable_array_map_string_values() {
    let out = compile_and_run(
        r#"<?php
$prefix = "a";
$starts = function(string $value) use ($prefix): int {
    return str_starts_with($value, $prefix) ? 1 : 0;
};
$result = array_map($starts, ["aa", "bb", "ab"]);
echo $result[0];
echo $result[1];
echo $result[2];
"#,
    );
    assert_eq!(out, "101");
}

#[test]
fn test_closure_array_filter() {
    let out = compile_and_run(
        r#"<?php
$evens = array_filter([1, 2, 3, 4, 5, 6], function($x) { return $x % 2 == 0; });
echo count($evens);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_captured_closure_array_filter() {
    let out = compile_and_run(
        r#"<?php
$limit = 4;
$filtered = array_filter([1, 4, 5, 9], function($x) use ($limit) { return $x > $limit; });
echo count($filtered);
foreach ($filtered as $value) { echo $value; }
"#,
    );
    assert_eq!(out, "259");
}

#[test]
fn test_captured_closure_variable_array_filter_string_values() {
    let out = compile_and_run(
        r#"<?php
$prefix = "a";
$starts = function(string $value) use ($prefix) {
    return str_starts_with($value, $prefix);
};
$filtered = array_filter(["aa", "bb", "ab"], $starts);
echo count($filtered);
foreach ($filtered as $value) { echo $value; }
"#,
    );
    assert_eq!(out, "2aaab");
}

#[test]
fn test_captured_closure_call_user_func() {
    let out = compile_and_run(
        r#"<?php
$base = 30;
$fn = function($x) use ($base) { return $base + $x; };
echo call_user_func($fn, 12);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_inline_captured_closure_call_user_func() {
    let out = compile_and_run(
        r#"<?php
$base = 9;
echo call_user_func(function($x) use ($base) { return $x * $base; }, 6);
"#,
    );
    assert_eq!(out, "54");
}

#[test]
fn test_arrow_function_array_filter() {
    let out = compile_and_run(
        r#"<?php
$big = array_filter([1, 5, 10, 15, 20], fn($x) => $x > 8);
echo count($big);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_closure_as_variable_then_call() {
    let out = compile_and_run(
        r#"<?php
$fn = function($x) { return $x + 1; };
$a = $fn(10);
$b = $fn(20);
echo $a;
echo $b;
"#,
    );
    assert_eq!(out, "1121");
}

#[test]
fn test_closure_no_params() {
    let out = compile_and_run(
        r#"<?php
$hello = function() { return 42; };
echo $hello();
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_arrow_no_params() {
    let out = compile_and_run(
        r#"<?php
$val = fn() => 99;
echo $val();
"#,
    );
    assert_eq!(out, "99");
}

#[test]
fn test_closure_array_reduce() {
    let out = compile_and_run(
        r#"<?php
$sum = array_reduce([1, 2, 3, 4], function($carry, $item) { return $carry + $item; }, 0);
echo $sum;
"#,
    );
    assert_eq!(out, "10");
}

// --- IIFE (Immediately Invoked Function Expression) ---

#[test]
fn test_iife_basic() {
    let out = compile_and_run(
        r#"<?php
echo (function() { return 42; })();
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_iife_with_args() {
    let out = compile_and_run(
        r#"<?php
echo (function($x) { return $x * 3; })(7);
"#,
    );
    assert_eq!(out, "21");
}

#[test]
fn test_iife_arrow() {
    let out = compile_and_run(
        r#"<?php
echo (fn($x) => $x + 100)(5);
"#,
    );
    assert_eq!(out, "105");
}

// --- Calling closures from array access ---

#[test]
fn test_closure_from_array_call() {
    let out = compile_and_run(
        r#"<?php
$fns = [function($x) { return $x * 10; }];
echo $fns[0](5);
"#,
    );
    assert_eq!(out, "50");
}

#[test]
fn test_closure_from_array_no_args() {
    let out = compile_and_run(
        r#"<?php
$fns = [function() { return 99; }];
echo $fns[0]();
"#,
    );
    assert_eq!(out, "99");
}

// --- Closure returning closure ---

#[test]
fn test_closure_returning_closure() {
    let out = compile_and_run(
        r#"<?php
$f = function() { return function() { return 99; }; };
$g = $f();
echo $g();
"#,
    );
    assert_eq!(out, "99");
}

#[test]
fn test_closure_returning_closure_with_args() {
    let out = compile_and_run(
        r#"<?php
$maker = function() { return function($x) { return $x * 3; }; };
$fn = $maker();
echo $fn(7);
"#,
    );
    assert_eq!(out, "21");
}
