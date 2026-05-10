//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types, named arguments direct calls and builtins, including named arguments reorder function call, named arguments use defaults for missing params, and named arguments closure call.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_named_arguments_reorder_function_call() {
    let out = compile_and_run(
        "<?php
        function describe($name, $age) {
            echo $name;
            echo \":\";
            echo $age;
        }
        describe(age: 30, name: \"Alice\");
        ",
    );
    assert_eq!(out, "Alice:30");
}

#[test]
fn test_named_arguments_use_defaults_for_missing_params() {
    let out = compile_and_run(
        "<?php
        function greet($name = \"world\", $suffix = \"!\") {
            echo $name . $suffix;
        }
        greet(suffix: \"?\");
        ",
    );
    assert_eq!(out, "world?");
}

#[test]
fn test_named_arguments_closure_call() {
    let out = compile_and_run(
        "<?php
        $f = function ($name, $age) {
            echo $name;
            echo \":\";
            echo $age;
        };
        $f(age: 30, name: \"Alice\");
        ",
    );
    assert_eq!(out, "Alice:30");
}

#[test]
fn test_named_arguments_first_class_callable_call() {
    let out = compile_and_run(
        "<?php
        function describe($name, $age) {
            echo $name;
            echo \":\";
            echo $age;
        }
        $f = describe(...);
        $f(age: 30, name: \"Alice\");
        ",
    );
    assert_eq!(out, "Alice:30");
}

#[test]
fn test_named_arguments_method_and_constructor_calls() {
    let out = compile_and_run(
        "<?php
        class User {
            public $name;
            public $age;

            public function __construct($name, $age = 18) {
                $this->name = $name;
                $this->age = $age;
            }

            public function describe($prefix, $suffix = \"!\") {
                echo $prefix . $this->name . \":\" . $this->age . $suffix;
            }
        }

        $user = new User(age: 30, name: \"Alice\");
        $user->describe(suffix: \"?\", prefix: \"user=\");
        ",
    );
    assert_eq!(out, "user=Alice:30?");
}

#[test]
fn test_named_arguments_static_method_call() {
    let out = compile_and_run(
        "<?php
        class Greeter {
            public static function hi($name, $punct = \"!\") {
                echo \"Hi \" . $name . $punct;
            }
        }
        Greeter::hi(punct: \"?\", name: \"Alice\");
        ",
    );
    assert_eq!(out, "Hi Alice?");
}

#[test]
fn test_named_arguments_builtin_call() {
    let out = compile_and_run(
        r#"<?php
echo strlen(string: "hello");
echo ":";
echo str_repeat(times: 3, string: "ha");
"#,
    );
    assert_eq!(out, "5:hahaha");
}

#[test]
fn test_named_arguments_builtin_case_insensitive_namespace_fallback() {
    let out = compile_and_run(
        r#"<?php
namespace Demo;
echo STRLEN(string: "abc");
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_named_arguments_builtin_uses_defaults_for_skipped_optional_params() {
    let out = compile_and_run(
        r#"<?php
echo number_format(num: 1234, thousands_separator: " ");
"#,
    );
    assert_eq!(out, "1 234");
}

#[test]
fn test_named_arguments_builtin_settype_reorders_call() {
    let out = compile_and_run(
        r#"<?php
$value = 42;
settype(type: "string", var: $value);
echo gettype($value) . ":" . $value;
"#,
    );
    assert_eq!(out, "string:42");
}

#[test]
fn test_named_arguments_builtin_mutating_array_arg_keeps_original_variable() {
    let out = compile_and_run(
        r#"<?php
$items = [3, 1, 2];
sort(array: $items);
foreach ($items as $item) {
    echo $item;
}
"#,
    );
    assert_eq!(out, "123");
}

#[test]
fn test_named_arguments_builtin_with_spread_prefix() {
    let out = compile_and_run(
        r#"<?php
$args = ["ha"];
echo str_repeat(...$args, times: 3);
"#,
    );
    assert_eq!(out, "hahaha");
}

#[test]
fn test_named_arguments_builtin_preserve_source_evaluation_order() {
    let out = compile_and_run(
        r#"<?php
function string_arg() {
    echo "s";
    return "ha";
}
function times_arg() {
    echo "t";
    return 3;
}
echo ":" . str_repeat(times: times_arg(), string: string_arg());
"#,
    );
    assert_eq!(out, "ts:hahaha");
}

#[test]
fn test_named_arguments_builtin_after_spread_evaluates_spread_once() {
    let out = compile_and_run(
        r#"<?php
function args() {
    echo "x";
    return ["ha"];
}
function times_arg() {
    echo "t";
    return 3;
}
echo ":" . str_repeat(...args(), times: times_arg());
"#,
    );
    assert_eq!(out, "xt:hahaha");
}

