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

/// Verifies basic anonymous function creation, assignment to variable, and invocation with one argument.
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

/// Verifies anonymous function with multiple parameters and a simple arithmetic body.
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

/// Verifies basic arrow function (`fn`) syntax with one parameter and multiplication body.
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

/// Verifies arrow function with a compound expression body (`$x * $x + 1`).
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

/// Regression for #300: arrow functions capture outer locals by value at definition time.
#[test]
fn test_arrow_function_captures_outer_local_by_value() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$f = fn() => $x;
$x = 2;
echo $f();
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies closure with typed parameter, return type annotation, and `use` clause capturing a string variable.
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

/// Verifies closure parameter and return type are both `string`, with passthrough returning the same value.
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

/// Verifies arrow function with typed `int` parameter and return type annotation.
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

/// Verifies immediately-invoked arrow function (IIFE) with return type annotation and no parameters.
#[test]
fn test_iife_arrow_return_type_annotation() {
    let out = compile_and_run(
        r#"<?php
echo (fn(): string => "ready")();
"#,
    );
    assert_eq!(out, "ready");
}

/// Verifies `array_map` with an anonymous closure using `array_map(function($x) { ... }, [...])` syntax.
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

/// Verifies `array_map` with a typed arrow function `fn(int $x): int => ...` passed as callable.
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

/// Verifies `array_map` with a closure that captures a variable via `use ($factor)`.
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

/// Verifies `array_map` where the callable closure is assigned to a variable before passing.
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

/// Verifies callback runtimes read by-value closure captures from descriptor storage
/// instead of rereading the current source variable after reassignment.
#[test]
fn test_captured_closure_variable_array_map_uses_descriptor_capture_after_reassign() {
    let out = compile_and_run(
        r#"<?php
$offset = 5;
$add = function(int $x) use ($offset): int {
    return $x + $offset;
};
$offset = 100;
$result = array_map($add, [1, 2]);
echo $result[0];
echo ":";
echo $result[1];
"#,
    );
    assert_eq!(out, "6:7");
}

/// Verifies string capture via `use ($prefix)` in a typed closure passed to `array_map`, producing string-concatenated output.
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

/// Verifies `str_starts_with` inside a captured closure passed to `array_map` with string array input.
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

/// Verifies `array_filter` with an anonymous closure returning even numbers.
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

/// Verifies `array_filter` with a captured `use ($limit)` closure comparing against a threshold.
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

/// Verifies `str_starts_with` inside a captured closure passed to `array_filter` with string array input.
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

/// Verifies `call_user_func` with a closure that captures a base value via `use ($base)`.
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

/// Verifies `call_user_func` with an inline immediately-created captured closure without intermediate variable assignment.
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

/// Verifies inline closure `call_user_func()` dispatch goes through a descriptor invoker.
#[test]
fn test_inline_closure_call_user_func_uses_descriptor_invoker() {
    let source = r#"<?php
$base = 9;
echo call_user_func(function(int $x) use ($base): int { return $x * $base; }, 6);
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "54");

    let dir = make_cli_test_dir("elephc_inline_closure_call_user_func_invoker");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("callable_invoker"),
        "inline call_user_func closure dispatch should route through descriptor invokers:\n{}",
        user_asm
    );
    let _ = fs::remove_dir_all(dir);
}

/// Verifies branch-selected captured callables route through `call_user_func()` descriptor invokers.
#[test]
fn test_call_user_func_complex_captured_callable_expr_uses_descriptor_invoker() {
    let source = r#"<?php
class Counter {
    public int $base = 0;

    public function add(int $n = 4): int {
        return $n + $this->base;
    }
}

$left = new Counter();
$left->base = 3;
$right = new Counter();
$right->base = 7;
$use_left = false;
echo call_user_func($use_left ? $left->add(...) : $right->add(...), 5);
echo ",";
echo call_user_func($use_left ? $left->add(...) : $right->add(...));
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "12,11");

    let dir = make_cli_test_dir("elephc_call_user_func_complex_callable_expr_invoker");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("callable_invoker"),
        "call_user_func branch-selected captured callable calls should route through descriptor invokers:\n{}",
        user_asm
    );
    let _ = fs::remove_dir_all(dir);
}

/// Verifies `call_user_func()` descriptor invokers preserve by-reference args for branch callables.
#[test]
fn test_call_user_func_complex_captured_callable_expr_preserves_by_ref_arg() {
    let source = r#"<?php
class Counter {
    public int $step = 0;

    public function bump(int &$n): void {
        $n = $n + $this->step;
    }
}

$left = new Counter();
$left->step = 3;
$right = new Counter();
$right->step = 7;
$use_left = false;
$value = 5;
call_user_func($use_left ? $left->bump(...) : $right->bump(...), $value);
echo $value;
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "12");
}

/// Verifies branch-selected captured first-class callables use descriptor invokers.
#[test]
fn test_direct_complex_captured_callable_expr_uses_descriptor_invoker() {
    let source = r#"<?php
class Counter {
    public int $base = 0;

    public function add(int $n): int {
        return $n + $this->base;
    }
}

$left = new Counter();
$left->base = 3;
$right = new Counter();
$right->base = 7;
$use_left = false;
echo ($use_left ? $left->add(...) : $right->add(...))(5);
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "12");

    let dir = make_cli_test_dir("elephc_direct_complex_callable_expr_invoker");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("callable_invoker"),
        "direct branch-selected captured callable calls should route through descriptor invokers:\n{}",
        user_asm
    );
    let _ = fs::remove_dir_all(dir);
}

/// Verifies direct descriptor calls with one spread source pass the source container through.
#[test]
fn test_direct_complex_captured_callable_expr_single_spread_uses_descriptor_invoker() {
    let source = r#"<?php
class Prefixer {
    public string $prefix = "";

    public function wrap(string $name, string $suffix = "!"): string {
        return $this->prefix . $name . $suffix;
    }
}

$left = new Prefixer();
$left->prefix = "L:";
$right = new Prefixer();
$right->prefix = "R:";
$use_left = false;
$args = ["Ada"];
echo ($use_left ? $left->wrap(...) : $right->wrap(...))(...$args);
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "R:Ada!");

    let dir = make_cli_test_dir("elephc_direct_complex_callable_expr_single_spread_invoker");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("callable_invoker"),
        "direct branch-selected single-spread callable calls should route through descriptor invokers:\n{}",
        user_asm
    );
    let _ = fs::remove_dir_all(dir);
}

/// Verifies direct descriptor calls with positional+spread args build invoker containers.
#[test]
fn test_direct_complex_captured_callable_expr_positional_spread_uses_descriptor_invoker() {
    let source = r#"<?php
class Prefixer {
    public string $prefix = "";

    public function wrap(string $name, string $suffix): string {
        return $this->prefix . $name . $suffix;
    }
}

$left = new Prefixer();
$left->prefix = "L:";
$right = new Prefixer();
$right->prefix = "R:";
$use_left = false;
$args = ["?"];
echo ($use_left ? $left->wrap(...) : $right->wrap(...))("Ada", ...$args);
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "R:Ada?");

    let dir = make_cli_test_dir("elephc_direct_complex_callable_expr_positional_spread_invoker");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("callable_invoker"),
        "direct branch-selected positional+spread callable calls should route through descriptor invokers:\n{}",
        user_asm
    );
    let _ = fs::remove_dir_all(dir);
}

/// Verifies branch-selected descriptor invokers preserve named arguments and defaults.
#[test]
fn test_direct_complex_captured_callable_expr_named_args_use_descriptor_invoker() {
    let source = r#"<?php
class Counter {
    public int $base = 0;

    public function add(int $n = 4): int {
        return $n + $this->base;
    }
}

$left = new Counter();
$left->base = 3;
$right = new Counter();
$right->base = 7;
$use_left = false;
echo ($use_left ? $left->add(...) : $right->add(...))(n: 5);
echo ",";
echo ($use_left ? $left->add(...) : $right->add(...))();
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "12,11");

    let dir = make_cli_test_dir("elephc_direct_complex_callable_expr_named_invoker");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("callable_invoker"),
        "direct branch-selected named callable calls should route through descriptor invokers:\n{}",
        user_asm
    );
    let _ = fs::remove_dir_all(dir);
}

/// Verifies branch-selected descriptor invokers accept spread prefixes followed by named args.
#[test]
fn test_direct_complex_captured_callable_expr_named_spread_args_use_descriptor_invoker() {
    let source = r#"<?php
class Counter {
    public int $base = 0;

    public function add(int $n = 4, int $scale = 1): int {
        return ($n * $scale) + $this->base;
    }
}

$left = new Counter();
$left->base = 3;
$right = new Counter();
$right->base = 7;
$use_left = false;
$args = [2];
echo ($use_left ? $left->add(...) : $right->add(...))(...$args, scale: 5);
echo ",";
$empty = [];
echo ($use_left ? $left->add(...) : $right->add(...))(...$empty, n: 6);
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "17,13");

    let dir = make_cli_test_dir("elephc_direct_complex_callable_expr_named_spread_invoker");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("callable_invoker"),
        "direct branch-selected named+spread callable calls should route through descriptor invokers:\n{}",
        user_asm
    );
    let _ = fs::remove_dir_all(dir);
}

/// Verifies stored branch-selected captured callables invoke through descriptor metadata.
#[test]
fn test_stored_branch_selected_captured_callable_variable_uses_descriptor_invoker() {
    let source = r#"<?php
class Prefixer {
    public string $prefix = "";

    public function wrap(string $name, string $suffix = "!"): string {
        return $this->prefix . $name . $suffix;
    }
}

$left = new Prefixer();
$left->prefix = "L:";
$right = new Prefixer();
$right->prefix = "R:";
$use_left = false;
$cb = $use_left ? $left->wrap(...) : $right->wrap(...);
echo $cb(name: "Ada");
echo ",";
echo $cb("Eve", suffix: "?");
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "R:Ada!,R:Eve?");

    let dir = make_cli_test_dir("elephc_stored_branch_callable_variable_invoker");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("callable_invoker"),
        "stored branch-selected callable variables should route through descriptor invokers:\n{}",
        user_asm
    );
    let _ = fs::remove_dir_all(dir);
}

/// Verifies stored descriptor calls preserve by-reference args through runtime signature metadata.
#[test]
fn test_stored_branch_selected_captured_callable_variable_preserves_by_ref_arg() {
    let source = r#"<?php
class Counter {
    public int $step = 0;

    public function bump(int &$n): void {
        $n = $n + $this->step;
    }
}

$left = new Counter();
$left->step = 3;
$right = new Counter();
$right->step = 7;
$use_left = false;
$cb = $use_left ? $left->bump(...) : $right->bump(...);
$value = 5;
$cb($value);
echo $value;
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "12");

    let dir = make_cli_test_dir("elephc_stored_branch_callable_variable_by_ref_invoker");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("callable_invoker"),
        "stored branch-selected callable variables with by-ref args should route through descriptor invokers:\n{}",
        user_asm
    );
    let _ = fs::remove_dir_all(dir);
}

/// Verifies stored untyped branch-selected callables use descriptor metadata for named args.
#[test]
fn test_stored_branch_selected_untyped_callable_variable_named_args_uses_descriptor_invoker() {
    let source = r#"<?php
class Calculator {
    public $base;

    public function __construct($base) {
        $this->base = $base;
    }

    public function scale($value = 1, $factor = 1) {
        return $this->base + ($value * $factor);
    }
}

$left = new Calculator(10);
$right = new Calculator(100);
$use_left = false;
$cb = $use_left ? $left->scale(...) : $right->scale(...);
echo $cb(value: 2, factor: 4);
$args = [2];
echo ",";
echo $cb(...$args, factor: 4);
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "108,108");

    let dir = make_cli_test_dir("elephc_stored_untyped_branch_callable_named_invoker");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("callable_invoker"),
        "stored untyped branch-selected callable variables with named args should route through descriptor invokers:\n{}",
        user_asm
    );
    let _ = fs::remove_dir_all(dir);
}

/// Verifies callable params with unknown signatures dereference named variable markers for by-value params.
#[test]
fn test_callable_param_unknown_signature_named_variable_arg_uses_descriptor_invoker() {
    let source = r#"<?php
function run(callable $cb): void {
    $name = "Ada";
    echo $cb(name: $name);
    echo ":";
    echo $name;
}

$cb = function(string $name): string {
    return "hi " . $name;
};

run($cb);
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "hi Ada:Ada");

    let dir = make_cli_test_dir("elephc_callable_param_unknown_named_value_invoker");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("callable_invoker"),
        "callable params with unknown named variable args should route through descriptor invokers:\n{}",
        user_asm
    );
    let _ = fs::remove_dir_all(dir);
}

/// Verifies callable params with unknown signatures preserve named by-reference variables.
#[test]
fn test_callable_param_unknown_signature_named_by_ref_arg_uses_descriptor_invoker() {
    let source = r#"<?php
function run(callable $cb): void {
    $value = 5;
    $cb(value: $value);
    echo $value;
}

$cb = function(int &$value): void {
    $value = $value + 7;
};

run($cb);
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "12");

    let dir = make_cli_test_dir("elephc_callable_param_unknown_named_ref_invoker");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("callable_invoker"),
        "callable params with unknown named by-ref args should route through descriptor invokers:\n{}",
        user_asm
    );
    let _ = fs::remove_dir_all(dir);
}

/// Verifies unknown callable params preserve named by-reference variables after a spread prefix.
#[test]
fn test_callable_param_unknown_signature_named_spread_by_ref_arg_uses_descriptor_invoker() {
    let source = r#"<?php
function run(callable $cb): void {
    $value = 5;
    $args = [];
    $cb(...$args, value: $value);
    echo $value;
}

$cb = function(int &$value): void {
    $value = $value + 11;
};

run($cb);
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "16");
}

/// Verifies unknown callable params preserve positional by-reference variables before a spread tail.
#[test]
fn test_callable_param_unknown_signature_positional_spread_by_ref_arg_uses_descriptor_invoker() {
    let source = r#"<?php
function run(callable $cb): void {
    $value = 5;
    $args = [];
    $cb($value, ...$args);
    echo $value;
}

$cb = function(int &$value): void {
    $value = $value + 13;
};

run($cb);
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "18");

    let dir = make_cli_test_dir("elephc_callable_param_unknown_positional_spread_ref_invoker");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("callable_invoker"),
        "callable params with positional+spread by-ref args should route through descriptor invokers:\n{}",
        user_asm
    );
    let _ = fs::remove_dir_all(dir);
}

/// Verifies unknown callable params preserve positional by-reference variables before named suffixes.
#[test]
fn test_callable_param_unknown_signature_named_spread_prefix_by_ref_arg_uses_descriptor_invoker() {
    let source = r#"<?php
function run(callable $cb): void {
    $value = 5;
    $args = [];
    $cb($value, ...$args, label: "done");
    echo ":" . $value;
}

$cb = function(int &$value, string $label): void {
    echo $label;
    $value = $value + 9;
};

run($cb);
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "done:14");

    let dir = make_cli_test_dir("elephc_callable_param_unknown_named_spread_prefix_ref_invoker");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("callable_invoker"),
        "callable params with named+spread prefix by-ref args should route through descriptor invokers:\n{}",
        user_asm
    );
    let _ = fs::remove_dir_all(dir);
}

/// Verifies receiver-bound callable params preserve named by-reference variables.
#[test]
fn test_callable_param_unknown_signature_method_named_by_ref_arg_uses_descriptor_invoker() {
    let source = r#"<?php
class Bumper {
    public $step;

    public function __construct($step) {
        $this->step = $step;
    }

    public function bump(&$value) {
        $value = $value + $this->step;
    }
}

function run(callable $cb): void {
    $value = 5;
    $cb(value: $value);
    echo $value;
}

$left = new Bumper(3);
$right = new Bumper(7);
$use_left = false;
$cb = $use_left ? $left->bump(...) : $right->bump(...);
run($cb);
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "12");
}

/// Verifies callable descriptors loaded from array elements invoke through runtime metadata.
#[test]
fn test_array_loaded_branch_selected_captured_callable_uses_descriptor_invoker() {
    let source = r#"<?php
class Prefixer {
    public string $prefix = "";

    public function wrap(string $name, string $suffix = "!"): string {
        return $this->prefix . $name . $suffix;
    }
}

$left = new Prefixer();
$left->prefix = "L:";
$right = new Prefixer();
$right->prefix = "R:";
$use_left = false;
$callbacks = [$use_left ? $left->wrap(...) : $right->wrap(...)];
echo $callbacks[0]("Ada");
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "R:Ada!");

    let dir = make_cli_test_dir("elephc_array_loaded_branch_callable_invoker");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("callable_invoker"),
        "array-loaded branch-selected callable descriptors should route through descriptor invokers:\n{}",
        user_asm
    );
    let _ = fs::remove_dir_all(dir);
}

/// Verifies `array_filter` with an arrow function predicate filtering values greater than 8.
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

/// Verifies closure stored in a variable and called multiple times, confirming each invocation is independent.
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

/// Verifies anonymous closure with no parameters that returns a constant integer.
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

/// Verifies arrow function with no parameters that returns a constant integer.
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

/// Verifies `array_reduce` with an anonymous closure summing a numeric array, using an initial carry value of 0.
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

/// Verifies immediately-invoked anonymous function expression (IIFE) returning a constant.
#[test]
fn test_iife_basic() {
    let out = compile_and_run(
        r#"<?php
echo (function() { return 42; })();
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies immediately-invoked anonymous function expression (IIFE) with one argument passed at call site.
#[test]
fn test_iife_with_args() {
    let out = compile_and_run(
        r#"<?php
echo (function($x) { return $x * 3; })(7);
"#,
    );
    assert_eq!(out, "21");
}

/// Verifies immediately-invoked arrow function (IIFE) with one argument passed at call site.
#[test]
fn test_iife_arrow() {
    let out = compile_and_run(
        r#"<?php
echo (fn($x) => $x + 100)(5);
"#,
    );
    assert_eq!(out, "105");
}

/// Verifies immediately-invoked closures can use named arguments and defaults.
#[test]
fn test_iife_named_args_and_defaults() {
    let out = compile_and_run(
        r#"<?php
echo (function(int $n = 4): int { return $n + 1; })(n: 5);
echo ",";
echo (function(int $n = 4): int { return $n + 1; })();
"#,
    );
    assert_eq!(out, "6,5");
}

// --- Calling closures from array access ---

/// Verifies closure stored in an array and invoked via array-access syntax `$fns[0](5)`.
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

/// Verifies parameterless closure stored in an array and invoked via array-access syntax `$fns[0]()`.
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

/// Verifies a closure that returns another closure, which is then invoked, confirming proper closure-of-closure codegen.
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

/// Verifies a closure factory that returns a closure accepting one argument, which is then called with a value.
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

// --- Closures auto-bind $this when defined in an instance method ---

/// Verifies a non-static closure defined in a method auto-captures `$this`, so
/// reading a property through `$this->prop` works without an explicit `use`.
#[test]
fn test_closure_in_method_auto_captures_this_property() {
    let out = compile_and_run(
        r#"<?php
class C {
    public int $v = 5;
    public function make() {
        return function() { return $this->v + 1; };
    }
}
$c = new C();
$f = $c->make();
echo $f();
"#,
    );
    assert_eq!(out, "6");
}

/// Verifies an arrow function defined in a method auto-captures `$this`.
#[test]
fn test_arrow_in_method_auto_captures_this() {
    let out = compile_and_run(
        r#"<?php
class C {
    public int $v = 5;
    public function make() {
        return fn() => $this->v * 10;
    }
}
$c = new C();
$f = $c->make();
echo $f();
"#,
    );
    assert_eq!(out, "50");
}

/// Verifies a captured `$this` can call instance methods inside the closure body.
#[test]
fn test_closure_in_method_calls_this_method() {
    let out = compile_and_run(
        r#"<?php
class C {
    public function greet() { return "hi"; }
    public function make() {
        return function() { return $this->greet() . "!"; };
    }
}
$c = new C();
$f = $c->make();
echo $f();
"#,
    );
    assert_eq!(out, "hi!");
}

/// Verifies a closure captures `$this` alongside an explicit `use($var)` capture.
#[test]
fn test_closure_captures_this_and_use_variable() {
    let out = compile_and_run(
        r#"<?php
class C {
    public int $base = 100;
    public function make($add) {
        return function() use ($add) { return $this->base + $add; };
    }
}
$c = new C();
$f = $c->make(7);
echo $f();
"#,
    );
    assert_eq!(out, "107");
}

/// Verifies the captured `$this` is the live object: mutations through the
/// closure persist across calls and are visible on the object.
#[test]
fn test_closure_mutates_this_property() {
    let out = compile_and_run(
        r#"<?php
class C {
    public int $n = 0;
    public function bump() {
        return function() { $this->n = $this->n + 1; return $this->n; };
    }
}
$c = new C();
$f = $c->bump();
echo $f(), $f(), $f();
"#,
    );
    assert_eq!(out, "123");
}

/// Verifies `$this` flows transitively into a nested closure defined inside an
/// outer closure: each level captures `$this` from the level above.
#[test]
fn test_nested_closures_share_this() {
    let out = compile_and_run(
        r#"<?php
class C {
    public int $v = 3;
    public function make() {
        return function() {
            $inner = fn() => $this->v * 2;
            return $inner();
        };
    }
}
$c = new C();
$f = $c->make();
echo $f();
"#,
    );
    assert_eq!(out, "6");
}

/// Verifies a closure that returns the captured `$this`'s state keeps the object
/// alive and readable after the defining method has returned.
#[test]
fn test_closure_reads_this_after_method_returns() {
    let out = compile_and_run(
        r#"<?php
class C {
    public string $name = "Ada";
    public function greeter() {
        return function() { return "Hi " . $this->name; };
    }
}
$c = new C();
$g = $c->greeter();
echo $g();
"#,
    );
    assert_eq!(out, "Hi Ada");
}

// --- Closure::bind / ->bindTo() rebind a closure's captured $this ---

/// Verifies `$closure->bindTo($newThis)` returns a closure whose `$this` is the
/// new receiver, leaving the original closure unchanged.
#[test]
fn test_closure_bindto_rebinds_this() {
    let out = compile_and_run(
        r#"<?php
class C {
    public int $v;
    public function __construct(int $v) { $this->v = $v; }
    public function getter() {
        return function() { return $this->v; };
    }
}
$c1 = new C(7);
$c2 = new C(99);
$f = $c1->getter();
$bound = $f->bindTo($c2);
echo $f(), $bound(), $f();
"#,
    );
    assert_eq!(out, "7997");
}

/// Verifies the static `Closure::bind($closure, $newThis)` form rebinds `$this`.
#[test]
fn test_closure_bind_static_form() {
    let out = compile_and_run(
        r#"<?php
class C {
    public int $v;
    public function __construct(int $v) { $this->v = $v; }
    public function getter() {
        return function() { return $this->v; };
    }
}
$c1 = new C(7);
$c2 = new C(50);
$f = $c1->getter();
$bound = Closure::bind($f, $c2);
echo $bound();
"#,
    );
    assert_eq!(out, "50");
}

/// Verifies the optional `$scope` argument is accepted by both bind spellings.
#[test]
fn test_closure_bind_accepts_scope_argument() {
    let out = compile_and_run(
        r#"<?php
class C {
    public int $v;
    public function __construct(int $v) { $this->v = $v; }
    public function getter() {
        return function() { return $this->v; };
    }
}
$c1 = new C(1);
$c2 = new C(42);
$f = $c1->getter();
$a = Closure::bind($f, $c2, C::class);
$b = $f->bindTo($c2, C::class);
echo $a(), " ", $b();
"#,
    );
    assert_eq!(out, "42 42");
}

/// Verifies `$closure->call($newThis, ...$args)` binds `$this` and invokes the
/// closure in one step, passing through the trailing arguments.
#[test]
fn test_closure_call_binds_and_invokes() {
    let out = compile_and_run(
        r#"<?php
class C {
    public int $v;
    public function __construct(int $v) { $this->v = $v; }
    public function adder() {
        return function(int $n) { return $this->v + $n; };
    }
}
$c1 = new C(7);
$c2 = new C(100);
$f = $c1->adder();
echo $f->call($c2, 5);   // 105 — bound to $c2
echo " ";
echo $f->call($c1, 1);   // 8   — bound to $c1
"#,
    );
    assert_eq!(out, "105 8");
}

// --- A closure defined outside a class may reference $this and be bound later ---

/// Verifies a top-level closure that references `$this` can be bound to an object
/// via `Closure::bind`, dispatching member access against the bound object.
#[test]
fn test_top_level_closure_bind_reads_property() {
    let out = compile_and_run(
        r#"<?php
class C {
    public int $x = 42;
}
$reader = function() { return $this->x; };
$bound = Closure::bind($reader, new C());
echo $bound();
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies the canonical scope-stealing pattern: a standalone closure bound to
/// an object reads a private property (visibility is permissive once bound).
#[test]
fn test_top_level_closure_bind_reads_private_property() {
    let out = compile_and_run(
        r#"<?php
class Account {
    private int $balance = 250;
}
$peek = function() { return $this->balance; };
$read = Closure::bind($peek, new Account(), Account::class);
echo $read();
"#,
    );
    assert_eq!(out, "250");
}

/// Verifies a top-level closure that calls a method on `$this` and takes an
/// argument, bound via both `bindTo` and `call`.
#[test]
fn test_top_level_closure_bind_method_and_call() {
    let out = compile_and_run(
        r#"<?php
class Greeter {
    public string $name = "Ada";
    public function hi(): string { return "Hi " . $this->name; }
}
$f = function(string $suffix) { return $this->hi() . $suffix; };
$bound = $f->bindTo(new Greeter());
echo $bound("!");
echo "|";
echo $f->call(new Greeter(), "?");
"#,
    );
    assert_eq!(out, "Hi Ada!|Hi Ada?");
}
