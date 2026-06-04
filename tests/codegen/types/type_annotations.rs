//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types type annotations, including example union types compiles and runs, typed array parameter, and typed callable parameter.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Uses checked-in example PHP fixtures through include_str! in addition to inline native-output assertions.

use super::*;

/// Compiles and runs the checked-in `examples/union-types/main.php` fixture and asserts stdout is "41:string:ready".
#[test]
fn test_example_union_types_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/union-types/main.php"));
    assert_eq!(out, "41:string:ready");
}

/// Verifies a function with a typed `array` parameter accepts an array literal and `count()` works at runtime.
#[test]
fn test_typed_array_parameter() {
    let out = compile_and_run(
        "<?php
        function total(array $values) {
            echo count($values);
        }
        total([1, 2, 3]);
        ",
    );
    assert_eq!(out, "3");
}

/// Verifies a function with a typed `callable` parameter accepts a first-class callable
/// and invokes it with an integer argument, returning the incremented result.
#[test]
fn test_typed_callable_parameter() {
    let out = compile_and_run(
        "<?php
        function apply(callable $fn) {
            echo $fn(1);
        }
        function plus_one($x) {
            return $x + 1;
        }
        apply(plus_one(...));
        ",
    );
    assert_eq!(out, "2");
}

/// Verifies a function with a typed `int &$x` by-ref parameter mutates the caller's variable
/// and the new value is observable after the call.
#[test]
fn test_typed_by_ref_parameter() {
    let out = compile_and_run(
        "<?php
        function bump(int &$x) {
            $x = $x + 1;
        }
        $value = 4;
        bump($value);
        echo $value;
        ",
    );
    assert_eq!(out, "5");
}

/// Verifies a method with a typed `array` parameter is callable on an instance and `count()` returns the expected value.
#[test]
fn test_typed_method_parameter() {
    let out = compile_and_run(
        "<?php
        class Box {
            public function size(array $items) {
                echo count($items);
            }
        }
        $box = new Box();
        $box->size([1, 2]);
        ",
    );
    assert_eq!(out, "2");
}

/// Verifies a constructor with a typed `int $id` parameter stores the value in a public property
/// accessible after object construction.
#[test]
fn test_typed_constructor_parameter() {
    let out = compile_and_run(
        "<?php
        class User {
            public $id;
            public function __construct(int $id) {
                $this->id = $id;
            }
        }
        $user = new User(42);
        echo $user->id;
        ",
    );
    assert_eq!(out, "42");
}

/// Verifies a typed parameter with a default value uses that default when the argument is omitted.
#[test]
fn test_typed_default_parameter_uses_default() {
    let out = compile_and_run(
        "<?php
        function add_ten(int $value = 10): int {
            return $value + 10;
        }
        echo add_ten();
        ",
    );
    assert_eq!(out, "20");
}

/// Verifies a typed parameter with a default value is overridden when an explicit argument is passed.
#[test]
fn test_typed_default_parameter_override() {
    let out = compile_and_run(
        "<?php
        function add_ten(int $value = 10): int {
            return $value + 10;
        }
        echo add_ten(5);
        ",
    );
    assert_eq!(out, "15");
}

/// Verifies a closure with a typed parameter with a default value uses the default when called
/// without arguments and overrides when an argument is passed.
#[test]
fn test_typed_closure_default_parameter() {
    let out = compile_and_run(
        "<?php
        $f = function (int $value = 10) {
            return $value + 1;
        };
        echo $f();
        echo \"|\";
        echo $f(4);
        ",
    );
    assert_eq!(out, "11|5");
}

/// Verifies a first-class callable created from a typed-parameter function works with the
/// default value when called without arguments and overrides when arguments are passed.
#[test]
fn test_typed_first_class_callable_default_parameter() {
    let out = compile_and_run(
        "<?php
        function add_ten(int $value = 10): int {
            return $value + 10;
        }
        $f = add_ten(...);
        echo $f();
        echo \"|\";
        echo $f(7);
        ",
    );
    assert_eq!(out, "20|17");
}

/// Verifies `call_user_func(add_ten(...))` uses the default value and `call_user_func("add_ten", 5)`
/// overrides it, matching PHP's behavior for first-class callable syntax.
#[test]
fn test_typed_call_user_func_default_parameter() {
    let out = compile_and_run(
        "<?php
        function add_ten(int $value = 10): int {
            return $value + 10;
        }
        echo call_user_func(add_ten(...));
        echo \"|\";
        echo call_user_func(\"add_ten\", 5);
        ",
    );
    assert_eq!(out, "20|15");
}

/// Verifies `call_user_func_array(add_ten(...), [])` uses the default value and
/// `call_user_func_array("add_ten", [5])` overrides it via the array argument.
#[test]
fn test_typed_call_user_func_array_default_parameter() {
    let out = compile_and_run(
        "<?php
        function add_ten(int $value = 10): int {
            return $value + 10;
        }
        echo call_user_func_array(add_ten(...), []);
        echo \"|\";
        echo call_user_func_array(\"add_ten\", [5]);
        ",
    );
    assert_eq!(out, "20|15");
}

/// Verifies descriptor invokers unbox boxed array arguments before calling
/// callbacks with declared `array` parameters.
#[test]
fn test_call_user_func_array_array_typed_callback_unboxes_mixed_arg() {
    let out = compile_and_run(
        "<?php
        function item_count(array $items): int {
            return count($items);
        }
        echo call_user_func_array(item_count(...), [[1, 2, 3]]);
        ",
    );
    assert_eq!(out, "3");
}

/// Verifies a closure with a typed `int $x` parameter compiles, runs, and produces the expected output.
#[test]
fn test_typed_closure_parameter() {
    let out = compile_and_run(
        "<?php
        $f = function (int $x) {
            echo $x + 1;
        };
        $f(41);
        ",
    );
    assert_eq!(out, "42");
}

/// Verifies a function with a declared `string` return type returns the string correctly.
#[test]
fn test_typed_function_return_value() {
    let out = compile_and_run(
        "<?php
        function label(): string {
            return \"ok\";
        }
        echo label();
        ",
    );
    assert_eq!(out, "ok");
}

/// Verifies a nullable typed `?int` parameter accepts both `null` and an integer,
/// and `is_null()` distinguishes them correctly in the function body.
#[test]
fn test_nullable_typed_parameter_accepts_null_and_int() {
    let out = compile_and_run(
        "<?php
        function show(?int $value): string {
            return is_null($value) ? \"null\" : (string) $value;
        }
        echo show(null);
        echo \"|\";
        echo show(7);
        ",
    );
    assert_eq!(out, "null|7");
}

/// Verifies a union typed `int|string` parameter accepts both an integer and a string,
/// and `gettype()` reports the correct runtime type for each.
#[test]
fn test_union_typed_parameter_accepts_multiple_types() {
    let out = compile_and_run(
        "<?php
        function show(int|string $value): string {
            return gettype($value) . \":\" . $value;
        }
        echo show(1);
        echo \"|\";
        echo show(\"ok\");
        ",
    );
    assert_eq!(out, "integer:1|string:ok");
}

/// Regression: an untyped parameter called from distinct sites with incompatible types
/// (`int` then `string`) must be inferred as a union (`Mixed` at the ABI) so each argument
/// keeps its own runtime type. Before the fix `f(5)` reported "other" because the integer
/// argument was coerced to a string at the call site to match the last-seen parameter type.
#[test]
fn test_untyped_parameter_heterogeneous_calls_infer_union() {
    let out = compile_and_run(
        "<?php
        function f($x): string { return is_int($x) ? \"int\" : \"other\"; }
        echo f(5), \"|\", f(\"s\");
        ",
    );
    assert_eq!(out, "int|other");
}

/// Regression: each argument to a heterogeneously-called untyped parameter keeps its own
/// runtime type. `gettype` reports `integer` for the int call and `string` for the string call;
/// before the fix the integer argument was coerced to a string, so both reported `string`.
#[test]
fn test_untyped_parameter_heterogeneous_calls_keep_runtime_type() {
    let out = compile_and_run(
        "<?php
        function f($x): string { return gettype($x); }
        echo f(5), \"|\", f(\"hello\");
        ",
    );
    assert_eq!(out, "integer|string");
}

/// Regression: a `string` argument to a heterogeneously-called untyped parameter is still a
/// real string at runtime, so `is_string`/`strlen` see it correctly rather than a boxed int.
#[test]
fn test_untyped_parameter_heterogeneous_calls_preserve_string_value() {
    let out = compile_and_run(
        "<?php
        function f($x): int { return is_string($x) ? strlen($x) : -1; }
        echo f(\"abc\"), \"|\", f(5);
        ",
    );
    assert_eq!(out, "3|-1");
}

/// Regression: untyped parameters called only with integers keep the `Int` fallback rather
/// than widening to a union, so existing int-only inference (and `is_int`) is preserved.
#[test]
fn test_untyped_parameter_homogeneous_int_calls_stay_int() {
    let out = compile_and_run(
        "<?php
        function f($x): string { return is_int($x) ? \"i\" : \"n\"; }
        echo f(1), f(2), f(3);
        ",
    );
    assert_eq!(out, "iii");
}

/// Regression: the same heterogeneous-call union inference applies to instance method
/// parameters, so each argument to a method called with incompatible types keeps its runtime
/// type. Before the fix the method parameter was specialized to the last-seen type.
#[test]
fn test_untyped_method_parameter_heterogeneous_calls_keep_runtime_type() {
    let out = compile_and_run(
        "<?php
        class C { public function t($x): string { return gettype($x); } }
        $c = new C();
        echo $c->t(5), \"|\", $c->t(\"hello\");
        ",
    );
    assert_eq!(out, "integer|string");
}

/// Regression: heterogeneous-call union inference also applies to static method parameters.
#[test]
fn test_untyped_static_method_parameter_heterogeneous_calls_keep_runtime_type() {
    let out = compile_and_run(
        "<?php
        class C { public static function t($x): string { return gettype($x); } }
        echo C::t(5), \"|\", C::t(\"hello\");
        ",
    );
    assert_eq!(out, "integer|string");
}

/// Verifies a nullable return type `?int` boxes an integer result and a `null` result,
/// with `is_null()` correctly identifying the null case at runtime.
#[test]
fn test_nullable_return_type_boxes_results() {
    let out = compile_and_run(
        "<?php
        function maybe(bool $flag): ?int {
            if ($flag) {
                return 7;
            }
            return null;
        }
        echo is_null(maybe(false)) ? \"null\" : \"value\";
        echo \"|\";
        echo maybe(true);
        ",
    );
    assert_eq!(out, "null|7");
}

/// Verifies a function with a declared return type that only throws an exception compiles
/// and runs correctly, with the exception caught and its message echoed.
#[test]
fn test_declared_return_type_allows_throw_only_body() {
    let out = compile_and_run(
        "<?php
        function fail(): string {
            throw new \\Exception(\"boom\");
        }
        try {
            echo fail();
        } catch (\\Exception $e) {
            echo $e->getMessage();
        }
        ",
    );
    assert_eq!(out, "boom");
}

/// Verifies a function with a declared return type that calls `exit` compiles and runs
/// without producing output after the exit.
#[test]
fn test_declared_return_type_allows_exit_only_body() {
    let out = compile_and_run(
        "<?php
        function bail(): int {
            exit(0);
        }
        echo \"before\";
        bail();
        echo \"after\";
        ",
    );
    assert_eq!(out, "before");
}

/// Verifies a function with a declared return type whose body is an infinite loop (`while(true)`)
/// or (`for(;;)`) compiles and runs to completion without hanging the test.
#[test]
fn test_declared_return_type_allows_infinite_loop_body() {
    let out = compile_and_run(
        "<?php
        function spin(): int {
            while (true) {
                echo \"\";
            }
        }
        function spin_for(): int {
            for (;;) {
                echo \"\";
            }
        }
        echo \"ok\";
        ",
    );
    assert_eq!(out, "ok");
}

/// Verifies a function with a declared return type whose body is an exhaustive `switch`
/// (all cases return) compiles and runs correctly for both case arms.
#[test]
fn test_declared_return_type_allows_exhaustive_switch_body() {
    let out = compile_and_run(
        "<?php
        function label(int $value): string {
            switch ($value) {
                case 1:
                    return \"one\";
                default:
                    return \"other\";
            }
        }
        echo label(1);
        echo \"|\";
        echo label(2);
        ",
    );
    assert_eq!(out, "one|other");
}

/// Verifies an arrow function with a nullable return type `?int` returns `null` or an integer
/// based on the boolean argument, and `is_null()` distinguishes them correctly.
#[test]
fn test_arrow_nullable_return_type_allows_null_value() {
    let out = compile_and_run(
        "<?php
        $maybe = fn(bool $ok): ?int => $ok ? 7 : null;
        echo is_null($maybe(false)) ? \"null\" : \"value\";
        echo \"|\";
        echo $maybe(true);
        ",
    );
    assert_eq!(out, "null|7");
}

/// Verifies a union return type `int|string` boxes results correctly: `gettype()` reports
/// `integer` for the int branch and the string branch is output as-is.
#[test]
fn test_union_return_type_boxes_results() {
    let out = compile_and_run(
        "<?php
        function choose(bool $flag): int|string {
            if ($flag) {
                return 7;
            }
            return \"ok\";
        }
        echo gettype(choose(true));
        echo \"|\";
        echo choose(false);
        ",
    );
    assert_eq!(out, "integer|ok");
}

/// Verifies a function with `mixed` parameter and return type accepts and returns any PHP value,
/// preserving type across the call.
#[test]
fn test_mixed_parameter_and_return_type() {
    let out = compile_and_run(
        "<?php
        function id(mixed $value): mixed {
            return $value;
        }
        echo gettype(id(\"ok\"));
        echo \"|\";
        echo id(7);
        ",
    );
    assert_eq!(out, "string|7");
}

/// Verifies `call_user_func_array` with a `?int` typed callback parameter accepts `null`
/// and an integer via spread array, confirming both paths work.
#[test]
fn test_call_user_func_array_with_nullable_callback_param() {
    let out = compile_and_run(
        "<?php
        function show(?int $value): string {
            return is_null($value) ? \"null\" : (string) $value;
        }
        echo call_user_func_array(show(...), [null]);
        echo \"|\";
        echo call_user_func_array(show(...), [7]);
        ",
    );
    assert_eq!(out, "null|7");
}

/// Verifies a nullable by-ref parameter `?int &$value` accepts a boxed typed local `?int`,
/// and assigning `null` inside the function clears the caller's variable.
#[test]
fn test_nullable_by_ref_parameter_accepts_boxed_typed_local() {
    let out = compile_and_run(
        "<?php
        function clear(?int &$value): void {
            $value = null;
        }
        ?int $value = 7;
        clear($value);
        echo is_null($value) ? \"null\" : \"value\";
        ",
    );
    assert_eq!(out, "null");
}
