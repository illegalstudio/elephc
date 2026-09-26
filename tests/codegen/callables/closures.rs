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

/// Resolves `Closure::fromCallable()` for a builtin function into a callable Closure value.
#[test]
fn test_closure_from_callable_builtin_function() {
    let out = compile_and_run(
        r#"<?php
$closure = Closure::fromCallable('strtoupper');
echo $closure('hello'), '|', get_class($closure), '|';
echo $closure instanceof Closure ? 'yes' : 'no';
"#,
    );
    assert_eq!(out, "HELLO|Closure|yes");
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

/// Verifies every native bind spelling preserves the hidden called-class capture
/// while replacing `$this`.
#[test]
fn test_closure_bind_consumers_preserve_late_static_class() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public string $value;
    const LABEL = "base";
    public function __construct(string $value) { $this->value = $value; }
    public function reader() {
        return function() { return $this->value . ":" . static::LABEL; };
    }
}
class Child extends Base {
    const LABEL = "child";
}
$source = new Child("source");
$replacement = new Child("replacement");
$f = $source->reader();
$staticBound = Closure::bind($f, $replacement);
$methodBound = $f->bindTo($replacement);
echo $staticBound(), "|", $methodBound(), "|", $f->call($replacement);
"#,
    );
    assert_eq!(out, "replacement:child|replacement:child|replacement:child");
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
/// an object reads a private property through the explicitly requested scope.
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

/// Verifies explicit scope works for both binding spellings without changing
/// the original closure or a separate binding that keeps global scope.
#[test]
fn test_top_level_closure_bind_private_scope_is_isolated() {
    let out = compile_and_run(
        r#"<?php
class Vault {
    private string $code = "open";
}
$peek = function() { return $this->code; };
$global = Closure::bind($peek, new Vault());
try {
    echo $global();
} catch (Error $error) {
    echo "denied|";
}
$static = Closure::bind($peek, new Vault(), Vault::class);
$method = $peek->bindTo(new Vault(), Vault::class);
$literal = Closure::bind(function() { return $this->code; }, new Vault(), Vault::class);
echo $static(), "|", $method(), "|", $literal();
"#,
    );
    assert_eq!(out, "denied|open|open|open");
}

/// Verifies try/catch callable tracking does not restore an entry closure after the local was
/// rebound inside the try body.
#[test]
fn test_scoped_closure_bind_after_try_uses_the_rebound_closure() {
    let out = compile_and_run(
        r#"<?php
class Vault {
    private string $code = "old";
    public string $label = "new";
}
$peek = function() { return $this->code; };
try {
    $peek = function() { return $this->label; };
    throw new Error("stop");
} catch (Error $error) {}
$bound = Closure::bind($peek, new Vault(), Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "new");
}

/// Verifies a dynamic eval mutation inside try prevents restoration of the entry closure fact.
#[test]
fn test_scoped_closure_bind_after_try_uses_dynamic_eval_rebinding() {
    let out = compile_and_run(
        r#"<?php
class Vault {
    private string $code = "old";
    public string $label = "new";
}
$peek = function() { return $this->code; };
$source = '$peek = function() { return $this->label; };';
try {
    eval($source);
} catch (Error $error) {}
$bound = Closure::bind($peek, new Vault(), Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "new");
}

/// Verifies a normal closure invocation inside try invalidates an entry callable fact when the
/// callee can replace that local through a by-reference capture.
#[test]
fn test_scoped_closure_bind_after_try_uses_rebinding_from_called_closure() {
    let out = compile_and_run(
        r#"<?php
class Vault {
    private string $code = "old";
    public string $label = "new";
}
$peek = function() { return $this->code; };
$mutate = function() use (&$peek): void {
    $peek = function() { return $this->label; };
};
try {
    $mutate();
} catch (Error $error) {}
$bound = Closure::bind($peek, new Vault(), Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "new");
}

/// Verifies a direct user function invalidates the callable local passed to its by-reference
/// parameter, without requiring dynamic invocation.
#[test]
fn test_scoped_closure_bind_after_try_uses_rebinding_from_ref_argument() {
    let out = compile_and_run(
        r#"<?php
function replace_callable(&$target): void {
    $target = function() { return $this->label; };
}
class Vault {
    private string $code = "old";
    public string $label = "new";
}
$peek = function() { return $this->code; };
try {
    replace_callable($peek);
} catch (Error $error) {}
$bound = Closure::bind($peek, new Vault(), Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "new");
}

/// Verifies a direct function's declared global alias invalidates the exact top-level callable
/// fact it can replace during a try body.
#[test]
fn test_scoped_closure_bind_after_try_uses_rebinding_from_global_function() {
    let out = compile_and_run(
        r#"<?php
function replace_global_callable(): void {
    global $peek;
    $peek = function() { return $this->label; };
}
class Vault {
    private string $code = "old";
    public string $label = "new";
}
$peek = function() { return $this->code; };
try {
    replace_global_callable();
} catch (Error $error) {}
$bound = Closure::bind($peek, new Vault(), Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "new");
}

/// Verifies global-write summaries follow direct user calls transitively. The wrapper declares no
/// global itself, but its callee replaces the top-level callable while the try body is active.
#[test]
fn test_scoped_closure_bind_after_try_uses_rebinding_from_wrapped_global_function() {
    let out = compile_and_run(
        r#"<?php
function replace_wrapped_global_callable(): void {
    global $peek;
    $peek = function() { return $this->label; };
}
function invoke_global_replacement(): void {
    replace_wrapped_global_callable();
}
class Vault {
    private string $code = "old";
    public string $label = "new";
}
$peek = function() { return $this->code; };
try {
    invoke_global_replacement();
} catch (Error $error) {}
$bound = Closure::bind($peek, new Vault(), Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "new");
}

/// Verifies releasing an object inside a try invalidates a global callable fact when its
/// destructor replaces that callable before the try join restores entry facts.
#[test]
fn test_scoped_closure_bind_after_try_sees_global_rebinding_from_destructor_cleanup() {
    let out = compile_and_run(
        r#"<?php
class DestructorCallableMutator {
    public function __destruct() {
        global $peek;
        $peek = function() { return $this->label; };
    }
}
class Vault {
    private string $code = "old";
    public string $label = "new";
}
$victim = new DestructorCallableMutator();
$peek = function() { return $this->code; };
try {
    unset($victim);
} catch (Error $error) {}
$bound = Closure::bind($peek, new Vault(), Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "new");
}

/// Verifies scalar and string cleanup inside a try does not invalidate an unrelated global
/// callable fact merely because another function declares that global name.
#[test]
fn test_scoped_closure_bind_after_try_survives_scalar_cleanup() {
    let out = compile_and_run(
        r#"<?php
function expose_callable_global(): void { global $peek; }
class Vault {
    private string $code = "open";
}
$peek = function() { return $this->code; };
try {
    $number = 1;
    $number = 2;
    $text = str_repeat('x', 2);
    $text = 'done';
    unset($number, $text);
} catch (Error $error) {}
$bound = Closure::bind($peek, new Vault(), Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "open");
}

/// Verifies indexed and associative overwrites invalidate a global callable fact when the
/// replaced element's destructor changes that callable before the try join.
#[test]
fn test_scoped_closure_bind_after_try_sees_container_overwrite_destructors() {
    let out = compile_and_run(
        r#"<?php
class ContainerOverwriteMutator {
    public string $label = "ignored";
    public function __destruct() {
        global $peek;
        $peek = function() { return $this->label; };
    }
}
function overwrite_victim(): mixed { return new ContainerOverwriteMutator(); }
function overwrite_container(array &$values): void { $values[0] = null; }
function invoke_container_overwrite(array &$values): void { overwrite_container($values); }
class Vault {
    private string $code = "old";
    public string $label = "new";
}
$vault = new Vault();
$indexed = [overwrite_victim()];
$peek = function() { return $this->code; };
try {
    $indexed[0] = null;
} catch (Error $error) {}
$bound = Closure::bind($peek, $vault, Vault::class);
echo $bound(), "|";
$hash = ["key" => overwrite_victim()];
$peek = function() { return $this->code; };
try {
    $hash["key"] = null;
} catch (Error $error) {}
$bound = Closure::bind($peek, $vault, Vault::class);
echo $bound(), "|";
$wrapped = [overwrite_victim()];
$peek = function() { return $this->code; };
try {
    invoke_container_overwrite($wrapped);
} catch (Error $error) {}
$bound = Closure::bind($peek, $vault, Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "new|new|new");
}

/// Verifies statically known and boxed associative unsets invalidate a global callable fact when
/// removing an element runs its destructor during a try body.
#[test]
fn test_scoped_closure_bind_after_try_sees_container_unset_destructors() {
    let out = compile_and_run(
        r#"<?php
class ContainerUnsetMutator {
    public string $label = "ignored";
    public function __destruct() {
        global $peek;
        $peek = function() { return $this->label; };
    }
}
function unset_victim(): mixed { return new ContainerUnsetMutator(); }
function boxed_unset_container(): mixed { return ["key" => unset_victim()]; }
function unset_container(array &$values): void { unset($values["key"]); }
function invoke_container_unset(array &$values): void { unset_container($values); }
class Vault {
    private string $code = "old";
    public string $label = "new";
}
$vault = new Vault();
$hash = ["key" => unset_victim()];
$peek = function() { return $this->code; };
try {
    unset($hash["key"]);
} catch (Error $error) {}
$bound = Closure::bind($peek, $vault, Vault::class);
echo $bound(), "|";
$boxed = boxed_unset_container();
$peek = function() { return $this->code; };
try {
    invoke_container_unset($boxed);
} catch (Error $error) {}
$bound = Closure::bind($peek, $vault, Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "new|new");
}

/// Verifies typed scalar container mutation does not discard an unrelated callable fact.
#[test]
fn test_scoped_closure_bind_after_try_survives_scalar_container_mutation() {
    let out = compile_and_run(
        r#"<?php
function expose_container_callable_global(): void { global $peek; }
class Vault {
    private string $code = "open";
}
$vault = new Vault();
$indexed = [1];
$hash = ["key" => 1];
$peek = function() { return $this->code; };
try {
    $indexed[0] = 2;
    $hash["key"] = 2;
    unset($hash["key"]);
} catch (Error $error) {}
$bound = Closure::bind($peek, $vault, Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "open");
}

/// Verifies restoring a registered handler invalidates a global callable fact when releasing the
/// handler's captured object runs its destructor in the try body.
#[test]
fn test_scoped_closure_bind_after_try_sees_handler_capture_destructor() {
    let out = compile_and_run(
        r#"<?php
class HandlerCaptureMutator {
    public string $label = "ignored";
    public function __destruct() {
        global $peek;
        $peek = function() { return $this->label; };
    }
}
class Vault {
    private string $code = "old";
    public string $label = "new";
}
$vault = new Vault();
$victim = new HandlerCaptureMutator();
$handler = function(int $level, string $message) use ($victim): bool { return false; };
set_error_handler($handler);
unset($handler, $victim);
$peek = function() { return $this->code; };
try {
    restore_error_handler();
} catch (Error $error) {}
$bound = Closure::bind($peek, $vault, Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "new");
}

/// Verifies a warning-producing instruction accounts for a user error handler that replaces a
/// global callable before control rejoins after a try body.
#[test]
fn test_scoped_closure_bind_after_try_sees_warning_handler_rebinding() {
    let out = compile_and_run(
        r#"<?php
class Vault {
    private string $code = "old";
    public string $label = "new";
}
function read_missing_key(array $values, string $key): mixed { return $values[$key]; }
function read_missing_key_wrapped(array $values, string $key): mixed {
    return read_missing_key($values, $key);
}
$vault = new Vault();
set_error_handler(function(int $level, string $message): bool {
    global $peek;
    $peek = function() { return $this->label; };
    return true;
});
$values = ["known" => 1];
$key = $argc > 0 ? "missing" : "other";
$peek = function() { return $this->code; };
try {
    $ignored = read_missing_key_wrapped($values, $key);
} catch (Error $error) {}
$bound = Closure::bind($peek, $vault, Vault::class);
restore_error_handler();
echo $bound();
"#,
    );
    assert_eq!(out, "new");
}

/// Verifies an output instruction accounts for an active output-buffer handler that replaces a
/// global callable when a chunk is flushed during the try body.
#[test]
fn test_scoped_closure_bind_after_try_sees_output_handler_rebinding() {
    let out = compile_and_run(
        r#"<?php
class Vault {
    private string $code = "old";
    public string $label = "new";
}
$vault = new Vault();
ob_start(function(string $buffer, int $phase): string {
    global $peek;
    $peek = function() { return $this->label; };
    return "";
}, 1);
$peek = function() { return $this->code; };
try {
    echo "x";
} catch (Error $error) {}
$bound = Closure::bind($peek, $vault, Vault::class);
ob_end_clean();
echo $bound();
"#,
    );
    assert_eq!(out, "new");
}

/// Verifies object string conversion invalidates a callable fact when `__toString` replaces it.
#[test]
fn test_scoped_closure_bind_after_try_sees_tostring_rebinding() {
    let out = compile_and_run(
        r#"<?php
class StringCallableMutator {
    public string $label = "ignored";
    public function __toString(): string {
        global $peek;
        $peek = function() { return $this->label; };
        return "converted";
    }
}
class Vault {
    private string $code = "old";
    public string $label = "new";
}
$vault = new Vault();
$value = new StringCallableMutator();
$peek = function() { return $this->code; };
try {
    $text = (string)$value;
} catch (Error $error) {}
$bound = Closure::bind($peek, $vault, Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "new");
}

/// Verifies a runtime-shaped nested write accounts for `ArrayAccess::offsetGet` mutating a
/// global callable while the try body is active.
#[test]
fn test_scoped_closure_bind_after_try_sees_array_access_fetch_rebinding() {
    let out = compile_and_run(
        r#"<?php
class AccessCallableMutator implements ArrayAccess {
    public string $label = "ignored";
    public function offsetExists(mixed $offset): bool { return true; }
    public function offsetGet(mixed $offset): mixed {
        global $peek;
        $peek = function() { return $this->label; };
        return [];
    }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void {}
}
function runtime_access_receiver(mixed $value): mixed { return $value; }
class Vault {
    private string $code = "old";
    public string $label = "new";
}
$vault = new Vault();
$access = new AccessCallableMutator();
$peek = function() { return $this->code; };
$receiver = runtime_access_receiver($access);
try {
    $receiver["slot"][] = 1;
} catch (Error $error) {}
$bound = Closure::bind($peek, $vault, Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "new");
}

/// Verifies a generator suspension prevents the try join from restoring the callable that was
/// current before caller code replaced its global cell.
#[test]
fn test_scoped_closure_bind_after_try_sees_rebinding_while_generator_is_suspended() {
    let out = compile_and_run(
        r#"<?php
class Vault {
    private string $code = "old";
    public string $label = "new";
}
function suspended_bind(): Generator {
    global $peek, $vault;
    $peek = function() { return $this->code; };
    try {
        yield 1;
    } catch (Error $error) {}
    $bound = Closure::bind($peek, $vault, Vault::class);
    echo $bound();
}
$vault = new Vault();
$generator = suspended_bind();
$generator->current();
$peek = function() { return $this->label; };
$generator->next();
"#,
    );
    assert_eq!(out, "new");
}

/// Verifies invoking a known closure with no by-reference captures preserves an unrelated
/// callable fact across a try join.
#[test]
fn test_scoped_closure_bind_after_try_survives_harmless_known_closure() {
    let out = compile_and_run(
        r#"<?php
class Vault {
    private string $code = "open";
}
$peek = function() { return $this->code; };
$noop = function(): void {};
try {
    $noop();
} catch (Error $error) {}
$bound = Closure::bind($peek, new Vault(), Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "open");
}

/// Verifies a known closure call invalidates a callable cell reachable through an object property
/// reference, even when that closure does not capture the callable local itself.
#[test]
fn test_scoped_closure_bind_after_try_sees_indirect_reference_mutation_from_closure() {
    let out = compile_and_run(
        r#"<?php
class ClosureAliasBox { public mixed $slot; }
class Vault {
    private string $code = "old";
    public string $label = "new";
}
$peek = function() { return $this->code; };
$box = new ClosureAliasBox();
$box->slot = $peek;
$peek =& $box->slot;
$mutate = function() use ($box): void {
    $box->slot = function() { return $this->label; };
};
try {
    $mutate();
} catch (Error $error) {}
$bound = Closure::bind($peek, new Vault(), Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "new");
}

/// Verifies a direct user function can mutate a callable through an object-property reference
/// without a by-reference argument, so all caller reference cells remain an opaque reachability
/// boundary for that call.
#[test]
fn test_scoped_closure_bind_after_try_sees_indirect_reference_mutation_from_function() {
    let out = compile_and_run(
        r#"<?php
class FunctionAliasBox { public mixed $slot; }
function mutate_alias(FunctionAliasBox $box): void {
    $box->slot = function() { return $this->label; };
}
class Vault {
    private string $code = "old";
    public string $label = "new";
}
$peek = function() { return $this->code; };
$box = new FunctionAliasBox();
$box->slot = $peek;
$peek =& $box->slot;
try {
    mutate_alias($box);
} catch (Error $error) {}
$bound = Closure::bind($peek, new Vault(), Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "new");
}

/// Verifies calls that cannot reach the closure local do not prevent the try join from restoring
/// its unchanged callable identity for a later scope-specialized bind.
#[test]
fn test_scoped_closure_bind_after_try_survives_unrelated_calls() {
    let out = compile_and_run(
        r#"<?php
function increment(int $value): int {
    return $value + 1;
}
class Vault {
    private string $code = "open";
    public string $label = "abc";
}
$peek = function() { return $this->code; };
try {
    $vault = new Vault();
    echo increment(strlen($vault->label)), "|";
} catch (Error $error) {}
$bound = Closure::bind($peek, new Vault(), Vault::class);
echo $bound();
"#,
    );
    assert_eq!(out, "4|open");
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

/// Verifies that `isset($this)` inside a `static` closure evaluates to `false`
/// because static closures have no `$this` binding (issue #359).
#[test]
fn test_static_closure_isset_this_returns_false() {
    let out = compile_and_run(
        r#"<?php
$f = static function(): bool { return isset($this); };
echo $f() ? "true" : "false";
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies that `isset($this)` inside a static closure defined in an instance
/// method still evaluates to `false` (issue #359).
#[test]
fn test_static_closure_isset_this_false_even_in_method() {
    let out = compile_and_run(
        r#"<?php
class C {
    public function m(): void {
        $f = static function(): bool { return isset($this); };
        echo $f() ? "true" : "false";
    }
}
(new C())->m();
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies that `isset($this, $x)` in a static closure returns `false` because
/// `$this` is unset, making the AND of all isset arguments false (issue #359).
#[test]
fn test_static_closure_isset_this_with_other_set_var() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$f = static function(): bool { return isset($this, $x); };
echo $f() ? "true" : "false";
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies that `isset($this)` in a non-static closure inside an instance
/// method evaluates to `true` because non-static closures auto-bind `$this`
/// (issue #359).
#[test]
fn test_non_static_closure_isset_this_true_in_method() {
    let out = compile_and_run(
        r#"<?php
class C {
    public function m(): bool {
        $f = function(): bool { return isset($this); };
        return $f();
    }
}
echo (new C())->m() ? "true" : "false";
"#,
    );
    assert_eq!(out, "true");
}

/// Verifies that a recursive closure capturing itself by reference with
/// `use(&$f)` compiles and runs correctly (issue #382).
#[test]
fn test_recursive_closure_factorial() {
    let out = compile_and_run(
        r#"<?php
$f = function (int $n) use (&$f): int {
    return $n <= 1 ? 1 : $n * $f($n - 1);
};
echo $f(5);
"#,
    );
    assert_eq!(out, "120");
}

/// Verifies a recursive closure computing Fibonacci numbers (issue #382).
#[test]
fn test_recursive_closure_fibonacci() {
    let out = compile_and_run(
        r#"<?php
$fib = function (int $n) use (&$fib): int {
    return $n < 2 ? $n : $fib($n - 1) + $fib($n - 2);
};
echo $fib(10);
"#,
    );
    assert_eq!(out, "55");
}

/// A PHP reference has no type: whatever a closure stores through a `use (&$x)` capture is what
/// the caller reads back, whether or not it matches what `$x` held when the closure was built.
///
/// The reference cell used to carry the captured variable's type, so a write of any other type
/// was reinterpreted through it — silently, with the wrong value rather than an error:
///
/// | program | php | before |
/// |---|---|---|
/// | `$b = 5;` then the closure writes `null` | `NULL` | `9223372036854775806` (the raw null sentinel read as an int) |
/// | `$c = null;` then the closure writes `7` | `7` | `NULL` — the write never landed |
/// | `$d = "s";` then the closure writes `null` | `NULL` | garbage bytes |
///
/// Only the same-type case worked, which is why this went unnoticed: it is the shape every
/// existing by-ref test used. The int-to-int row below is kept as the control — it must stay
/// correct through any future narrowing of the widening rule.
///
/// An int-to-STRING row is deliberately absent. A `use (&$e)` capture makes `$e`
/// reference-aliased, so `$e = 1; $e = "grown";` stays a hard `cannot reassign $e from int to
/// string` here even in permissive mode — a depth-0 retype only warns and re-binds when NO
/// reference can still reach the old cell. Pinning it would exercise that eligibility rule
/// rather than anything about how references carry a written-back value. Reassigning to `null`
/// is allowed, which is what makes these four rows reachable.
#[test]
fn test_by_ref_capture_writes_back_a_different_type() {
    let out = compile_and_run(
        r#"<?php
$a = 5;
(function () use (&$a) { $a = 9; })();
var_dump($a);
$b = 5;
(function () use (&$b) { $b = null; })();
var_dump($b);
$c = null;
(function () use (&$c) { $c = 7; })();
var_dump($c);
$d = "s";
$f = function () use (&$d) { $d = null; };
$f();
var_dump($d);
"#,
    );
    assert_eq!(out, "int(9)\nNULL\nint(7)\nNULL\n");
}

/// The accumulator is the overwhelmingly common by-ref capture, and it must keep answering
/// correctly whatever the cell's representation is. Pinned separately from the type-change
/// matrix above so a narrowing that spares this shape from widening has a witness of its own.
#[test]
fn test_by_ref_capture_accumulates_across_calls() {
    let out = compile_and_run(
        r#"<?php
$total = 0;
$add = function (int $n) use (&$total): void { $total += $n; };
for ($i = 0; $i < 500; $i++) { $add($i); }
echo $total;
"#,
    );
    assert_eq!(out, "124750");
}

/// The accumulator crossing `PHP_INT_MAX` is the SAME defect as the null write above, reached
/// by a different pair of types — and the one that shows the widening is not a tax on a working
/// program but the price of representing what PHP stores.
///
/// PHP promotes an overflowing sum to float, so `$total += $n` stores `int|float`, not `int`.
/// A cell typed from the captured `int` cannot hold that, and the sum wrapped around instead:
///
/// | call | php | before |
/// |---|---|---|
/// | first | `int(9223372036854775807)` | `int(9223372036854775807)` |
/// | second | `float(9.2233720368548E+18)` | `int(-9223372036854775808)` |
/// | third | `float(9.2233720368548E+18)` | `int(-9223372036854775798)` |
///
/// Silent, and it needs no `null` and no string anywhere — just arithmetic that leaves the
/// integer range, which is why an accumulator is exactly the shape most likely to meet it.
///
/// The CONTROL is what makes the cause unambiguous, so it is part of the same fixture: the
/// identical `+= 1` outside any closure promoted to float correctly even before the fix. The
/// arithmetic was never wrong — only the trip through a reference cell typed from the captured
/// `int` was, which is the same defect as the null row above reached by another pair of types.
#[test]
fn test_by_ref_capture_accumulator_promotes_to_float_on_overflow() {
    let out = compile_and_run(
        r#"<?php
$total = PHP_INT_MAX - 1;
$add = function (int $n) use (&$total): void { $total += $n; };
$add(1);
var_dump($total);
$add(1);
var_dump($total);
$plain = PHP_INT_MAX - 1;
$plain += 1;
var_dump($plain);
$plain += 1;
var_dump($plain);
"#,
    );
    assert_eq!(
        out,
        "int(9223372036854775807)\nfloat(9.223372036854776E+18)\n\
         int(9223372036854775807)\nfloat(9.223372036854776E+18)\n"
    );
}


/// Regression for #567: an explicit `null` argument must not close an untyped closure
/// parameter to null.
///
/// A closure's parameter specialization is FINAL — unlike a function's, it never widens to a
/// union — so the type the first call picks is the only one every later call may use. `Void`
/// is the one candidate no later call can ever satisfy, and adopting it turned
///
/// ```php
/// $f = function ($v = null) { … };
/// $f(null);
/// $f(5);          // error[7:4]: callable $f parameter $v expects Void, got Int
/// ```
///
/// into a compile error where PHP prints `nx`. PHP does not infer parameter types from
/// earlier calls: `= null` makes the parameter optional, and an untyped parameter keeps
/// accepting everything.
///
/// It also removed an asymmetry between two spellings of the same call — `$f()` and `$f(null)`
/// pass the same value, and only the second one closed the parameter. Both spellings are in
/// the fixture, next to each other, for that reason.
///
/// The reordered pair (`$f(7)` then `$f(null)`) and the direct-function pair are the controls
/// the issue asks for: neither had the defect, and both are asserted here so the fix cannot be
/// a swap of which order is broken.
///
/// The arrow-function row is written inline instead of calling `show()`, because a closure
/// whose body is `return <user function call>;` loses its return type for an unrelated reason
/// (#1028).
///
/// Every expectation is the host PHP 8.5.10 output for the same fixture.
#[test]
fn test_explicit_null_call_leaves_an_untyped_closure_parameter_open() {
    let out = compile_and_run(
        r#"<?php
class Tag { public function __construct(public string $name) {} }

function show($v): string
{
    if ($v === null) { return "null"; }
    if (is_array($v)) { return "array(" . count($v) . ")"; }
    if ($v instanceof Tag) { return "Tag:" . $v->name; }
    if (is_bool($v)) { return $v ? "true" : "false"; }
    return gettype($v) . ":" . $v;
}

$toInt = function ($v = null) { echo show($v), "|"; };
$toInt(null);
$toInt(5);

$toStr = function ($v = null) { echo show($v), "|"; };
$toStr(null);
$toStr("s");

$toArr = function ($v = null) { echo show($v), "|"; };
$toArr(null);
$toArr([1, 2, 3]);

$toObj = function ($v = null) { echo show($v), "|"; };
$toObj(null);
$toObj(new Tag("t"));

$toFloat = function ($v = null) { echo show($v), "|"; };
$toFloat(null);
$toFloat(1.5);

$toBool = function ($v = null) { echo show($v), "|"; };
$toBool(null);
$toBool(true);

$reordered = function ($v = null) { echo show($v), "|"; };
$reordered(7);
$reordered(null);

$omitted = function ($v = null) { echo show($v), "|"; };
$omitted();
$omitted(9);

$nothing = null;
$viaVar = function ($v = null) { echo show($v), "|"; };
$viaVar($nothing);
$viaVar(11);

$used = function ($v = null) { echo $v === null ? "n" : $v + 1, "|"; };
$used(null);
$used(41);
$used(null);

$pair = function ($a = null, $b = null) { echo show($a), ",", show($b), "|"; };
$pair(null, null);
$pair(3, "x");

$middle = function ($a, $b) { echo show($a), ",", show($b), "|"; };
$middle(1, null);
$middle(2, "z");

$arrow = fn ($v = null) => $v === null ? "null" : gettype($v) . ":" . $v;
echo $arrow(null), "|", $arrow(13), "|";

function plain($v = null): void { echo show($v), "|"; }
plain(null);
plain(17);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "null|integer:5|",
            "null|string:s|",
            "null|array(3)|",
            "null|Tag:t|",
            "null|double:1.5|",
            "null|true|",
            "integer:7|null|",
            "null|integer:9|",
            "null|integer:11|",
            "n|42|n|",
            "null,null|integer:3,string:x|",
            "integer:1,null|integer:2,string:z|",
            "null|integer:13|",
            "null|integer:17|",
        )
    );
}


/// A `: void` function's result is a `null` argument like any other, and must not fix an
/// untyped closure parameter either.
///
/// PHP has no separate "void value": calling a `void` function yields `null`, and passing it
/// on is ordinary code that runs. It is included here because the `Void` exclusion above is
/// written on the TYPE, not on the syntax — `$f(nothing())` and `$f(null)` reach it the same
/// way, and they must behave the same way, which is what these rows assert rather than assume.
///
/// The fourth pair is a `void` call into a parameter that also has a `= null` default: the two
/// nulls come from different places and still leave the parameter open.
///
/// Every expectation is the host PHP 8.5.10 output for the same fixture.
#[test]
fn test_void_call_argument_leaves_an_untyped_closure_parameter_open() {
    let out = compile_and_run(
        r#"<?php
function nothing(): void {}

$f = function ($x) { var_dump($x); };
$f(nothing());
$f(1);

$g = function ($x) { var_dump($x); };
$g(nothing());
$g("s");

$h = function ($x) { var_dump($x); };
$h(nothing());
$h(nothing());

$k = function ($x = null) { var_dump($x); };
$k(nothing());
$k(2.5);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "NULL\nint(1)\n",
            "NULL\nstring(1) \"s\"\n",
            "NULL\nNULL\n",
            "NULL\nfloat(2.5)\n",
        )
    );
}


/// A temporary passed to a CLOSURE must be released by the call, exactly as one passed to a
/// named function, a method or a static method already was.
///
/// `release_owned_call_arg_temporaries` is called from every other call lowering; the
/// static-callable closure arm never called it. The argument therefore outlived the call and its
/// destructor never ran — observable php semantics, not merely a leak:
///
/// ```php
/// $f = fn($a) => 7;
/// $f(new D());   // php prints the destructor here; elephc printed nothing
/// ```
///
/// The same closure RETURNING its parameter was always correct, because returning hands the
/// value out as the result for the caller to release — which is what masked this.
#[test]
fn test_temporary_passed_to_a_closure_is_destroyed_by_the_call() {
    let out = compile_and_run(
        r#"<?php
class D { public function __destruct() { echo "d"; } }
$f = fn($a) => 7;
$g = function ($a) { return 7; };
$keep = fn($a) => $a;
echo "[";
$f(new D());
echo "|";
$g(new D());
echo "|";
$keep(new D());
echo "]";
"#,
    );
    assert_eq!(out, "[d|d|d]");
}

/// The capture operands appended after the arguments belong to the closure, not to the call, so
/// releasing the arguments must not reach them: a captured object stays alive across calls and is
/// destroyed once, when the closure itself goes.
#[test]
fn test_closure_captures_survive_releasing_the_call_arguments() {
    let out = compile_and_run(
        r#"<?php
class D {
    public $tag;
    public function __construct($tag) { $this->tag = $tag; }
    public function __destruct() { echo $this->tag; }
}
$held = new D("H");
$f = function ($a) use ($held) { return 1; };
echo "[";
$f(new D("1"));
$f(new D("2"));
echo "]";
"#,
    );
    assert_eq!(out, "[12]H");
}


/// A temporary passed to a closure through the DESCRIPTOR INVOKER — which is what a call inside
/// any loop, and every builtin callback, uses — must be released once the callee returns.
///
/// The invoker increfs each by-value argument on the callee's behalf and nothing released it, so
/// the argument outlived the call and its destructor never ran. Straight-line calls resolve the
/// callee statically and take a different path, which is why this needs the loop.
#[test]
fn test_temporary_passed_through_the_descriptor_invoker_is_destroyed() {
    let out = compile_and_run(
        r#"<?php
class D { public $t; public function __construct($t) { $this->t = $t; } public function __destruct() { echo $this->t; } }
$f = fn($a) => 7;
echo "[";
for ($i = 1; $i <= 3; $i++) { $f(new D($i)); }
echo "]";
"#,
    );
    assert_eq!(out, "[123]");
}

/// The one case a blanket release would break: a callee that HANDS THE ARGUMENT BACK.
///
/// There the invoker's retained reference is exactly what makes the result owned, so releasing it
/// would free a live value — measured, removing the retain outright fails this with
/// `heap debug detected bad refcount`. The release is skipped when the result IS that argument.
#[test]
fn test_closure_returning_its_argument_keeps_it_alive_through_the_invoker() {
    let out = compile_and_run(
        r#"<?php
class D { public $t; public function __construct($t) { $this->t = $t; } public function __destruct() { echo $this->t; } }
$keep = fn($a) => $a;
echo "[";
for ($i = 1; $i <= 2; $i++) {
    $held = $keep(new D($i));
    echo "-";
}
echo "]";
"#,
    );
    assert_eq!(out, "[-1-]2");
}

/// One reference per callback invocation is what `array_filter`, `array_map` and `usort` were
/// each losing — three blocks per call on a three-element array, eight per `usort`. At this scale
/// a single leaked reference per invocation would be unmissable.
#[test]
fn test_callback_builtins_do_not_leak_a_reference_per_invocation() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [4, 1, 3, 2];
$n = 0;
for ($i = 0; $i < 200; $i++) {
    $n += count(array_filter($a, fn($x) => $x > 1));
    $n += count(array_map(fn($x) => $x + 1, $a));
    $b = [3, 1, 2];
    usort($b, fn($x, $y) => $x <=> $y);
    $n += count($b);
}
echo $n;
"#,
    );
    assert_eq!(out.stdout, "2000", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}


/// The invoker's post-call argument releases must not run while the result is still a REGISTER PAIR.
///
/// Every release is a call, and a call preserves nothing. Only a counted pointer fits in the
/// integer result register: a `Str` travels as a pointer/length pair (x1/x2 on aarch64, rax/rdx on
/// x86_64) and a `Float` in its own register, so whatever half is not saved is left to whatever the
/// release helper happened to leave behind.
///
/// The invoker answers this by ORDER rather than by saving: `emit_boxed_invoker_return` turns the
/// result into one Mixed cell pointer, and only then does `InvokerArgumentOwners::finish_return`
/// run the releases — which save that single pointer through a frame slot. So the assertion is the
/// order: the boxing allocation precedes the first release call. Flip the two and a string result
/// is back to crossing a call as a pair.
///
/// It does not corrupt today only because the reachable release helpers are register-frugal —
/// `__rt_decref_array` touches x0 and x9..x11 and returns without calling anything while the
/// refcount stays positive — which is an accident of their bodies, not a contract. The zero-count
/// path tail-calls `__rt_array_free_deep`, which is not frugal.
#[test]
fn test_invoker_preserves_a_string_result_across_retained_argument_releases() {
    let source = r#"<?php
$f = function (array $r) { return "n" . count($r); };
$a = call_user_func_array($f, [[1, 2, 3]]);
$b = call_user_func_array($f, [[4, 5]]);
echo $a, "|", $b, "|", strlen($a), strlen($b);
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "n3|n2|22");

    let dir = make_cli_test_dir("elephc_invoker_string_result_release");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    let invoker = user_asm
        .split("runtime callable invoker")
        .nth(1)
        .unwrap_or("")
        .to_string();
    let boxing = invoker
        .find("__rt_heap_alloc")
        .unwrap_or_else(|| panic!("the invoker must box its result:\n{}", invoker));
    let release = invoker
        .find("__rt_decref_any")
        .unwrap_or_else(|| panic!("the invoker must release its retained arguments:\n{}", invoker));
    assert!(
        boxing < release,
        "the invoker must box the string result into one Mixed pointer BEFORE the \
         retained-argument releases run; boxing at {} follows the first release at {}:\n{}",
        boxing,
        release,
        invoker
    );
    let _ = fs::remove_dir_all(dir);
}
