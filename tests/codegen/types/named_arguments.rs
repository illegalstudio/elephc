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
fn test_spread_only_uses_default_for_unpacked_optional_param() {
    let out = compile_and_run(
        r#"<?php
function show($a, $b = 99) {
    echo $a . ":" . $b;
}
show(...[10]);
"#,
    );
    assert_eq!(out, "10:99");
}

#[test]
fn test_spread_only_positional_prefix_uses_default_for_optional_tail() {
    let out = compile_and_run(
        r#"<?php
function show($a, $b = 99) {
    echo $a . ":" . $b;
}
show(10, ...[]);
"#,
    );
    assert_eq!(out, "10:99");
}

#[test]
fn test_assoc_spread_literal_maps_string_keys_to_named_args() {
    let out = compile_and_run(
        r#"<?php
function show($a, $b = 99) {
    echo $a . ":" . $b;
}
show(...["a" => 10]);
"#,
    );
    assert_eq!(out, "10:99");
}

#[test]
fn test_assoc_spread_literal_preserves_key_order_for_named_args() {
    let out = compile_and_run(
        r#"<?php
function show($a, $b) {
    echo $a . ":" . $b;
}
show(...["b" => 20, "a" => 10]);
"#,
    );
    assert_eq!(out, "10:20");
}

#[test]
fn test_assoc_spread_literal_mixes_numeric_and_string_keys() {
    let out = compile_and_run(
        r#"<?php
function show($a, $b) {
    echo $a . ":" . $b;
}
show(...[0 => 10, "b" => 20]);
"#,
    );
    assert_eq!(out, "10:20");
}

#[test]
fn test_assoc_spread_literal_for_builtin_call() {
    let out = compile_and_run(
        r#"<?php
echo str_repeat(...["string" => "ha", "times" => 3]);
"#,
    );
    assert_eq!(out, "hahaha");
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

#[test]
fn test_named_arguments_preserve_source_evaluation_order() {
    let out = compile_and_run(
        r#"<?php
function mark($label, $value) {
    echo $label;
    return $value;
}
function sum2($a, $b) {
    echo ":";
    echo $a + $b;
}
sum2(b: mark("b", 2), a: mark("a", 1));
"#,
    );
    assert_eq!(out, "ba:3");
}

#[test]
fn test_named_arguments_after_spread_evaluate_spread_once() {
    let out = compile_and_run(
        r#"<?php
function args() {
    echo "x";
    return [10, 20];
}
function last() {
    echo "c";
    return 30;
}
function sum3($a, $b, $c) {
    echo ":";
    echo $a + $b + $c;
}
sum3(...args(), c: last());
"#,
    );
    assert_eq!(out, "xc:60");
}

#[test]
fn test_named_arguments_after_multiple_spreads() {
    let out = compile_and_run(
        r#"<?php
function first() {
    echo "a";
    return [1];
}
function second() {
    echo "b";
    return [2];
}
function last() {
    echo "c";
    return 3;
}
function sum3($a, $b, $c) {
    echo ":";
    echo $a + $b + $c;
}
sum3(...first(), ...second(), c: last());
"#,
    );
    assert_eq!(out, "abc:6");
}

#[test]
fn test_named_arguments_after_spread_evaluate_later_named_before_runtime_error() {
    let out = compile_and_run_capture(
        r#"<?php
function args() {
    echo "s";
    return [1, 2, 99];
}
function last() {
    echo "c";
    return 30;
}
function sum3($a, $b, $c) {
    echo $a + $b + $c;
}
sum3(...args(), c: last());
"#,
    );
    assert!(!out.success);
    assert_eq!(out.stdout, "sc");
    assert!(out.stderr.contains("Fatal error: named argument spread length mismatch"));
}

#[test]
fn test_named_arguments_unknown_variadic_named_args_keep_string_keys() {
    let out = compile_and_run(
        r#"<?php
function show($head, ...$rest) {
    foreach ($rest as $key => $value) {
        echo $key . "=" . $value . ";";
    }
}
show(head: 1, extra: 2);
"#,
    );
    assert_eq!(out, "extra=2;");
}

#[test]
fn test_named_arguments_variadic_mixes_positional_and_named_extra_args() {
    let out = compile_and_run(
        r#"<?php
function show($head, ...$rest) {
    foreach ($rest as $key => $value) {
        echo $key . "=" . $value . ";";
    }
}
show(1, 2, extra: 3);
"#,
    );
    assert_eq!(out, "0=2;extra=3;");
}
