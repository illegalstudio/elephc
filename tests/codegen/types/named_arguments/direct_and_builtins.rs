//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types, named arguments direct calls and builtins, including named arguments reorder function call, named arguments use defaults for missing params, and named arguments closure call.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies named arguments can reorder parameters; `describe(age: 30, name: "Alice")` outputs "Alice:30".
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

/// Verifies named arguments use parameter defaults for omitted arguments; `greet(suffix: "?")` outputs "world?".
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

/// Verifies named arguments work on closure calls; `$f(age: 30, name: "Alice")` outputs "Alice:30".
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

/// Verifies named arguments work on first-class callable calls;
/// `$f = describe(...); $f(age: 30, name: "Alice")` outputs "Alice:30".
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

/// Verifies named arguments work on instance method calls and constructor calls;
/// `new User(age: 30, name: "Alice")` and `$user->describe(suffix: "?", prefix: "user=")` output "user=Alice:30?".
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

/// Verifies named arguments work on static method calls; `Greeter::hi(punct: "?", name: "Alice")` outputs "Hi Alice?".
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

/// Verifies named arguments work on builtins `strlen` and `str_repeat`; outputs "5:hahaha".
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

/// Verifies that calling a builtin with an all-caps name inside a namespace falls back correctly;
/// `STRLEN(string: "abc")` outputs "3".
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

/// Verifies `number_format(num: 1234, thousands_separator: " ")` uses defaults for omitted optional params.
#[test]
fn test_named_arguments_builtin_uses_defaults_for_skipped_optional_params() {
    let out = compile_and_run(
        r#"<?php
echo number_format(num: 1234, thousands_separator: " ");
"#,
    );
    assert_eq!(out, "1 234");
}

/// Verifies `settype(type: "string", var: $value)` reorders call args correctly; outputs "string:42".
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

/// Verifies that `sort(array: $items)` mutates the caller's array variable in place and iteration
/// produces the sorted order "123".
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

/// Verifies named arguments work after a spread prefix; `str_repeat(...$args, times: 3)` outputs "hahaha".
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

/// Verifies source evaluation order is preserved for builtin named args;
/// `str_repeat(times: times_arg(), string: string_arg())` outputs "ts:hahaha" (strings before times).
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

/// Verifies that a trailing comma after the last call argument (PHP 7.3+) and after the last
/// parameter (PHP 8.0+) is accepted across every call/declaration surface — top-level function,
/// method, static method, `new`, and closure — by summing values; outputs "11" (3+2+2+2+2).
#[test]
fn test_trailing_comma_across_call_surfaces() {
    let out = compile_and_run(
        r#"<?php
function add($a, $b,) {
    return $a + $b;
}
class Calc {
    public function sum($a, $b,) {
        return $a + $b;
    }
    public static function smul($a, $b,) {
        return $a * $b;
    }
}
class Pair {
    public $total;
    public function __construct($a, $b,) {
        $this->total = $a + $b;
    }
}
$c = new Calc();
$p = new Pair(1, 1,);
$mul = function ($a, $b,) {
    return $a * $b;
};
echo add(1, 2,) + $c->sum(1, 1,) + Calc::smul(1, 2,) + $p->total + $mul(1, 2,);
"#,
    );
    assert_eq!(out, "11");
}

/// Verifies that a spread argument is evaluated exactly once when followed by named arguments;
/// `str_repeat(...args(), times: times_arg())` outputs "xt:hahaha" (x printed once from args, t from times_arg).
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

