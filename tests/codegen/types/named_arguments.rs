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
fn test_named_arguments_after_spread_for_user_function() {
    let out = compile_and_run(
        r#"<?php
function sum3($a, $b, $c) {
    return $a + $b + $c;
}
$args = [10, 20];
echo sum3(...$args, c: 30);
"#,
    );
    assert_eq!(out, "60");
}

#[test]
fn test_named_arguments_after_spread_uses_default_for_unpacked_gap() {
    let out = compile_and_run(
        r#"<?php
function sum3($a, $b = 2, $c = 3) {
    return $a + $b + $c;
}
$args = [10];
echo sum3(...$args, c: 30);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_named_arguments_after_spread_rejects_short_spread() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function sum3($a, $b, $c) {
    return $a + $b + $c;
}
$args = [10];
echo sum3(...$args, c: 30);
"#,
    );
    assert!(err.contains("Fatal error: named argument spread length mismatch"));
}

#[test]
fn test_named_arguments_after_spread_rejects_overwrite() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function sum3($a, $b, $c) {
    return $a + $b + $c;
}
$args = [10, 20, 99];
echo sum3(...$args, c: 30);
"#,
    );
    assert!(err.contains("Fatal error: named argument spread length mismatch"));
}
