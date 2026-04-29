use super::*;

#[test]
fn test_first_class_callable_named_function_indirect_call() {
    let out = compile_and_run(
        r#"<?php
function triple($n) {
    return $n * 3;
}

$fn = triple(...);
echo $fn(7);
"#,
    );
    assert_eq!(out, "21");
}

#[test]
fn test_first_class_callable_builtin_used_in_array_map() {
    let out = compile_and_run(
        r#"<?php
$len = strlen(...);
echo $len("tool");
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_first_class_callable_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
function bump(&$n) {
    $n = $n + 1;
}

$fn = bump(...);
$value = 7;
$fn($value);
echo $value;
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_first_class_callable_alias_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
function bump(&$n) {
    $n = $n + 1;
}

$f = bump(...);
$g = $f;
$value = 7;
$g($value);
echo $value;
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_closure_alias_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
$f = function (&$x) {
    $x = $x + 1;
};

$g = $f;
$value = 7;
$g($value);
echo $value;
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_first_class_callable_variable_used_in_array_map() {
    let out = compile_and_run(
        r#"<?php
function double($n) {
    return $n * 2;
}

$fn = double(...);
$values = array_map($fn, [1, 2, 3]);
echo $values[0];
echo ":";
echo $values[2];
"#,
    );
    assert_eq!(out, "2:6");
}

#[test]
fn test_first_class_callable_untyped_function_accepts_string_args() {
    let out = compile_and_run(
        r#"<?php
function greet($name) {
    return "Hello " . $name;
}

$f = greet(...);
echo $f("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_first_class_callable_direct_call_user_func() {
    let out = compile_and_run(
        r#"<?php
echo call_user_func(strlen(...), "hello");
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_call_user_func_first_class_callable_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
function bump(&$n) {
    $n = $n + 1;
}

$f = bump(...);
$value = 5;
call_user_func($f, $value);
echo $value;
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_call_user_func_closure_alias_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
$f = function (&$x) {
    $x = $x + 1;
};
$g = $f;
$value = 5;
call_user_func($g, $value);
echo $value;
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_instance_method_preserves_multiple_byref_array_params() {
    let out = compile_and_run(
        r#"<?php
class Foo {
    public function bar(array &$a, array &$b): void {
        $a[0] = 1;
        $b[0] = 2;
    }
}

$x = [0];
$y = [0];
$foo = new Foo();
$foo->bar($x, $y);
echo $x[0];
echo $y[0];
"#,
    );
    assert_eq!(out, "12");
}

#[test]
fn test_first_class_callable_variadic_function_call() {
    let out = compile_and_run(
        r#"<?php
function count_args(...$xs) {
    echo count($xs);
}

$f = count_args(...);
$f(1, 2, 3);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_closure_variadic_call() {
    let out = compile_and_run(
        r#"<?php
$f = function (...$xs) {
    echo count($xs);
};

$f(1, 2, 3);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_first_class_callable_variadic_with_regular_param() {
    let out = compile_and_run(
        r#"<?php
function head_and_count($a, ...$rest) {
    echo $a;
    echo ":";
    echo count($rest);
}

$f = head_and_count(...);
$f(7, 8, 9);
"#,
    );
    assert_eq!(out, "7:2");
}

#[test]
fn test_first_class_callable_builtin_count_accepts_string_arrays() {
    let out = compile_and_run(
        r#"<?php
$f = count(...);
$xs = ["a", "b"];
echo $f($xs);
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_first_class_callable_builtin_count_accepts_assoc_arrays() {
    let out = compile_and_run(
        r#"<?php
$f = count(...);
$xs = ["a" => 1, "b" => 2];
echo $f($xs);
"#,
    );
    assert_eq!(out, "2");
}
