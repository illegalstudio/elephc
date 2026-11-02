//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of callables closures, including closure basic, closure multiple params, and arrow function basic.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

/// Verifies `get_debug_type()` names a BOXED callable. A closure or first-class callable reaching
/// a `mixed` slot is carried as a callable descriptor, a tag `gettype()` already routes to its
/// object arm and `instanceof \Closure` already answers true for — but which `get_debug_type()`
/// left out of its dispatch, so it fell through to the null arm and answered "null".
/// PHP outputs "Closure,object,obj,callable,closure" for both shapes, and keeps naming a string
/// callable "string" and an array callable "array".
#[test]
fn test_get_debug_type_names_a_boxed_callable() {
    let out = compile_and_run(
        r#"<?php
class Svc { public function run(int $n): int { return $n; } }
function double(int $n): int { return $n * 2; }

function probe(mixed $c): string {
    return implode(',', [
        get_debug_type($c),
        gettype($c),
        is_object($c) ? 'obj' : 'notobj',
        is_callable($c) ? 'callable' : 'notcallable',
        $c instanceof \Closure ? 'closure' : 'notclosure',
    ]);
}

echo probe(static fn (int $n): int => $n), '|';
echo probe(double(...)), '|';
echo get_debug_type('double'), '|';
echo get_debug_type([new Svc(), 'run']);
"#,
    );
    assert_eq!(
        out,
        "Closure,object,obj,callable,closure|Closure,object,obj,callable,closure|string|array"
    );
}

/// Verifies a `callable` value satisfies a `string|array|object` destination. PHP has no
/// `callable` property type, so code that stores one declares the three shapes a callable can
/// actually take at run time — Symfony's `ControllerEvent::$controller` is exactly
/// `private string|array|object $controller;`, written straight from a `callable $controller`
/// parameter. PHP outputs "Closure=5|Closure=8".
#[test]
fn test_callable_satisfies_a_string_array_object_property() {
    let out = compile_and_run(
        r#"<?php
class Holder {
    private string|array|object $controller;

    public function __construct(callable $controller) {
        $this->controller = $controller;
    }

    public function describe(): string { return get_debug_type($this->controller); }

    public function run(int $n): int {
        $c = $this->controller;

        return $c($n);
    }
}

function double(int $n): int { return $n * 2; }

$closure = new Holder(static fn (int $n): int => $n + 1);
echo $closure->describe(), '=', $closure->run(4), '|';
$named = new Holder(double(...));
echo $named->describe(), '=', $named->run(4);
"#,
    );
    assert_eq!(out, "Closure=5|Closure=8");
}

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

/// Verifies that untyped closure parameters retain gradual array access semantics.
#[test]
fn test_untyped_closure_parameter_supports_array_access() {
    let out = compile_and_run(
        r#"<?php
$classic = function ($value) { return $value[1]; };
$arrow = fn ($value) => $value[2];
echo $classic([10, 20, 30]), ":", $arrow([10, 20, 30]);
"#,
    );
    assert_eq!(out, "20:30");
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

/// Verifies a local assigned before its first arrow-body read is not captured from the parent scope.
#[test]
fn test_arrow_function_assignment_defines_local_before_read() {
    let out = compile_and_run(
        r#"<?php
$format = static fn($name) => match ($name) {
    'parent' => ($parent = 'base') ? $parent . '!' : 'none',
    default => 'other',
};
echo $format('parent');
"#,
    );
    assert_eq!(out, "base!");
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

/// Verifies `instanceof Closure` keeps the callable descriptor representation for calls and
/// callable-typed argument passing after narrowing a union.
#[test]
fn test_instanceof_closure_narrowing_remains_callable() {
    let out = compile_and_run(
        r#"<?php
function invoke_narrowed_callable(callable $callback, string $value): string {
    return $callback($value);
}

function invoke_closure_or_string(Closure|string $candidate): string {
    if (!$candidate instanceof Closure) {
        return $candidate;
    }

    return $candidate('direct') . ':' . invoke_narrowed_callable($candidate, 'passed');
}

$callback = static fn (string $value): string => strtoupper($value);
echo invoke_closure_or_string($callback);
"#,
    );
    assert_eq!(out, "DIRECT:PASSED");
}

/// Verifies a closure carried by a mixed loader resource remains a PHP `Closure` object.
#[test]
fn test_mixed_closure_instanceof_dispatches_delegating_loader() {
    let out = compile_and_run(
        r#"<?php
final class ClosureLoader {
    public function supports(mixed $resource, ?string $type = null): bool {
        return $resource instanceof Closure;
    }

    public function load(mixed $resource, ?string $type = null): string {
        return $resource();
    }
}

final class Resolver {
    public function __construct(private array $loaders) {}

    public function resolve(mixed $resource, ?string $type = null): object|false {
        foreach ($this->loaders as $loader) {
            if ($loader->supports($resource, $type)) {
                return $loader;
            }
        }

        return false;
    }
}

final class DelegatingLoader {
    public function __construct(private Resolver $resolver) {}

    public function load(mixed $resource, ?string $type = null): string {
        if (false === $loader = $this->resolver->resolve($resource, $type)) {
            return 'miss';
        }

        return $loader->load($resource, $type);
    }
}

echo (new DelegatingLoader(new Resolver([new ClosureLoader()])))->load(fn (): string => 'ok');
"#,
    );

    assert_eq!(out, "ok");
}

/// Verifies a callable selected from a gradual union is validated and invoked at runtime.
#[test]
fn test_gradual_union_callable_invocation() {
    let out = compile_and_run(
        r#"<?php
$candidate = $argc > 0 ? static fn (string $value): string => strtoupper($value) : null;
echo $candidate('ready');
"#,
    );
    assert_eq!(out, "READY");
}

/// Verifies a gradual union can supply a runtime-validated spread to an indirect callable.
#[test]
fn test_gradual_union_callable_spread() {
    let out = compile_and_run(
        r#"<?php
$callback = static fn (int $left, int $right): int => $left + $right;
$args = $argc > 0 ? [20, 22] : null;
echo $callback(...$args);
"#,
    );
    assert_eq!(out, "42");
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

/// Verifies a descriptor call passes through a sole spread stored behind a Mixed array slot.
#[test]
fn test_direct_callable_expr_single_mixed_spread_uses_descriptor_invoker() {
    let out = compile_and_run(
        r#"<?php
$add = $argc > 0
    ? static fn(int $left, int $right): int => $left + $right
    : static fn(int $left, int $right): int => $left - $right;
$state = ["callable" => $add, "args" => [20, 22], "marker" => 1];
$app = $state["callable"];
$args = $state["args"];
echo $app(...$args);
"#,
    );
    assert_eq!(out, "42");
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

/// Verifies `array_reduce()` with a non-`int` carry and a callback returned from a method, which
/// is Laravel's middleware `Pipeline::then()`, plus string, array and default-`null` carries.
///
/// Three defects stood in the way, all general:
/// - the checker typed `array_reduce()` as a fixed `int` (the native reducer's register), so
///   `$pipeline('x')` was refused "not a callable (got Int)";
/// - a closure RETURNED from a function or method reported its untyped parameters as `int`, so
///   `array_reduce(…, $this->carry(), $destination)` was refused "callback parameter $stack
///   expects Int, got Callable", and `$f = mk(); $f("str", 2)` "expects Int, got Str";
/// - a typed array with a non-`int` carry still took the native integer reducer.
///
/// Reference value captured from php 8.5.10.
#[test]
fn test_array_reduce_closure_carry_and_returned_closure_parameters() {
    let out = compile_and_run(
        r#"<?php
function mk() { return function ($x, $y) { return "got:" . $x . "/" . $y; }; }
$f = mk();
echo $f("str", 2), "\n";

class Pipeline {
    protected function carry() {
        return function ($stack, $pipe) {
            return function ($passable) use ($stack, $pipe) { return $pipe($passable, $stack); };
        };
    }
    public function then(Closure $destination, array $pipes) {
        $pipeline = array_reduce(array_reverse($pipes), $this->carry(), function ($passable) use ($destination) {
            return $destination($passable);
        });
        return $pipeline('x');
    }
}
$pipes = [fn ($v, $next) => "a(" . $next($v) . ")", fn ($v, $next) => "b(" . $next($v) . ")"];
echo (new Pipeline())->then(fn ($v) => "dest($v)", $pipes), "\n";
echo array_reduce(['a', 'b', 'c'], fn ($carry, $v) => $carry . $v, '>'), "|";
echo json_encode(array_reduce([1, 2, 3], fn ($carry, $v) => [...$carry, $v * 2], [])), "|";
echo array_reduce([1, 2, 3], fn ($carry, $v) => $carry + $v, 10), "|";
var_dump(array_reduce([], fn ($carry, $v) => $carry));
"#,
    );
    assert_eq!(out, "got:str/2\na(b(dest(x)))\n>abc|[2,4,6]|16|NULL\n");
}

/// Verifies closures stored in an array stay callable when the array is read through a `mixed`
/// parameter.
///
/// An array literal of closures was never stamped with the callable element tag (10), so every
/// `mixed` reader boxed each closure as an int and invoking it fataled "mixed value is not
/// callable". The boxed-carry `array_reduce()` helper iterates exactly such a parameter.
///
/// Reference value captured from php 8.5.10.
#[test]
fn test_closures_in_an_array_stay_callable_through_a_mixed_parameter() {
    let out = compile_and_run(
        r#"<?php
function red(mixed $array, callable $cb, mixed $init) {
    $c = $init;
    foreach ($array as $v) { $c = $cb($c, $v); }
    return $c;
}
$pipes = [fn ($v) => "a($v)", fn ($v) => "b($v)"];
echo red($pipes, function ($carry, $pipe) { return $pipe($carry); }, 'x'), "\n";
function first(mixed $array) { foreach ($array as $v) { return $v; } }
$f = first($pipes);
echo $f('y'), "\n";
echo is_callable(first($pipes)) ? "callable\n" : "not callable\n";
"#,
    );
    assert_eq!(out, "b(a(x))\na(y)\ncallable\n");
}

/// Verifies a call through a property-held closure is not bound to an unrelated closure literal
/// lowered earlier in the same function.
///
/// Lowering a closure literal parks its static binding for the next consumer, and an arrow
/// function passed as a method ARGUMENT is never consumed. The `finally` block's
/// `($this->fin)($this->passable)` then took that stale binding: a direct call to the arrow
/// function with the other ternary arm's captures, refused only because those captures did not
/// dominate the call ("UseNotDominated"). This is Laravel's `Pipeline::then()`.
///
/// Reference value captured from php 8.5.10.
#[test]
fn test_property_closure_call_does_not_take_a_stale_closure_binding() {
    let out = compile_and_run(
        r#"<?php
class Db { public function transaction(Closure $f) { return "tx(" . $f() . ")"; } }
class Pipe {
    public $fin = null;
    public $tx = 0;
    public $passable = 'p';
    public function then(Closure $pipeline) {
        try {
            return $this->tx !== 0
                ? (new Db())->transaction(fn () => $pipeline($this->passable))
                : $pipeline($this->passable);
        } finally {
            if ($this->fin) {
                ($this->fin)($this->passable);
            }
        }
    }
}
$p = new Pipe();
echo $p->then(fn ($v) => "d($v)"), "\n";
$p->fin = function ($v) { echo "finally($v)\n"; };
$p->tx = 1;
echo $p->then(fn ($v) => "d($v)"), "\n";
"#,
    );
    assert_eq!(out, "d(p)\nfinally(p)\ntx(d(p))\n");
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

/// Verifies an object-held closure preserves a nested associative map returned from a foreach.
///
/// Configuration builders commonly retain normalizer closures and call them later through a
/// property. The nested assignment and the array return must retain the same map rather than
/// degrading its child value to null at the callable boundary.
#[test]
fn test_object_held_closure_returns_nested_associative_map() {
    let out = compile_and_run(
        r#"<?php
class ClosureMapNormalizer {
    private \Closure $normalizer;

    public function __construct() {
        $this->normalizer = static function (array $values): array {
            foreach ($values as $key => $value) {
                $values[$key]['services'] = $value;
            }

            return $values;
        };
    }

    public function normalize(array $values): array {
        return ($this->normalizer)($values);
    }
}

$result = (new ClosureMapNormalizer())->normalize(['App\\' => []]);
echo get_debug_type($result['App\\']['services'] ?? null);
"#,
    );
    assert_eq!(out, "array");
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

/// A free-function closure whose only `$this` reads sit inside multi-argument `echo` (parsed as a
/// statement sequence) must still take a bound receiver once `Closure::bind` gives it one.
#[test]
fn test_bound_closure_reads_this_inside_multi_argument_echo() {
    let out = compile_and_run(
        r#"<?php
function make(): \Closure { return function () { echo "[", $this->c, "]", $this->p("x"); return 1; }; }
class R {
    private string $c = "UTF-8";
    private function p(string $n): string { return "<" . $n . ">"; }
    public function run(\Closure $f): void { \Closure::bind($f, $this, R::class)(); }
}
(new R())->run(make());
"#,
    );
    assert_eq!(out, "[UTF-8]<x>");
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

/// Both bind spellings retain the new receiver after its source variable is unset.
#[test]
fn test_closure_bind_retains_receiver_after_unset_with_clean_heap() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class BoundOwner {
    public int $v;
    public function __construct(int $v) { $this->v = $v; }
    public function getter() { return function() { return $this->v; }; }
}
$original = new BoundOwner(7);
$getter = $original->getter();
$receiver = new BoundOwner(99);
$first = $getter->bindTo($receiver);
$second = Closure::bind($getter, $receiver);
unset($receiver);
echo $first(), "|", $second(), "|", $getter();
unset($first, $second, $getter, $original);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "99|99|7");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
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

/// Verifies an explicit null-receiver bind scope controls lexical `self` static-property access.
#[test]
fn test_closure_bind_null_receiver_rebinds_static_scope() {
    let out = compile_and_run(
        r#"<?php
final class BoundStaticScope {
    private static ?array $formats = ["html"];

    public static function reset(): void {
        $reset = Closure::bind(static fn () => self::$formats = null, null, self::class);
        $reset();
    }

    public static function state(): string {
        return null === self::$formats ? "reset" : "set";
    }
}
BoundStaticScope::reset();
echo BoundStaticScope::state();
"#,
    );
    assert_eq!(out, "reset");
}

/// Verifies a null-receiver scope bind preserves ordinary lexical captures.
#[test]
fn test_closure_bind_null_receiver_preserves_use_captures() {
    let out = compile_and_run(
        r#"<?php
final class CapturedStaticScope {
}
$base = 40;
$bound = Closure::bind(function () use ($base): int {
    return $base + 2;
}, null, CapturedStaticScope::class);
echo $bound();
"#,
    );
    assert_eq!(out, "42");
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

/// Verifies a non-static closure created in a static method can receive `$this` when bound later.
#[test]
fn test_static_method_returns_bindable_closure_using_this() {
    let out = compile_and_run(
        r#"<?php
class StaticClosureFactory {
    public static function make(): Closure {
        return function(string $suffix): string { return $this->label . $suffix; };
    }
}

class StaticClosureReceiver {
    public string $label = "bound";
}

$closure = StaticClosureFactory::make();
$bound = $closure->bindTo(new StaticClosureReceiver());
echo $bound("!");
echo ":";
echo $closure->call(new StaticClosureReceiver(), "?");
"#,
    );
    assert_eq!(out, "bound!:bound?");
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

/// Verifies a closure retains its lexical class for a `self::` static-method call.
#[test]
fn test_closure_retains_lexical_class_for_self_static_method_call() {
    let out = compile_and_run(
        r#"<?php
class ClosureStaticMethodOwner {
    private static function label(): string { return 'owner'; }
    public static function callback(): Closure { return static fn (): string => self::label(); }
}
echo ClosureStaticMethodOwner::callback()();
"#,
    );
    assert_eq!(out, "owner");
}

/// Verifies a closure retains its lexical class for private `self::$property` access.
#[test]
fn test_closure_retains_lexical_class_for_self_static_property() {
    let out = compile_and_run(
        r#"<?php
class ClosureStaticPropertyOwner {
    private static string $label = 'property';
    public static function callback(): Closure { return static fn (): string => self::$label; }
}
echo ClosureStaticPropertyOwner::callback()();
"#,
    );
    assert_eq!(out, "property");
}

/// Verifies a closure forwards the defining call's late-static class id to `static::class`.
#[test]
fn test_closure_forwards_late_static_class_id() {
    let out = compile_and_run(
        r#"<?php
class ClosureStaticBase {
    public static function callback(): Closure { return static fn (): string => static::class; }
}
class ClosureStaticChild extends ClosureStaticBase {}
echo ClosureStaticChild::callback()();
"#,
    );
    assert_eq!(out, "ClosureStaticChild");
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

/// A closure that replaces ITSELF through its own by-reference capture, whether the variable is a
/// plain local or a reference to a fresh array element (`$closure = &$this->optimized[$e][]`, as
/// Symfony's `EventDispatcher::optimizeListeners()` builds its lazy-listener wrappers).
///
/// The self-capture's parameter was typed `callable`, while the cell behind it is a boxed `Mixed`
/// once the body reassigns it, so the body stored a raw descriptor into the box and the next call
/// read it as one: a segfault on the first replacement.
#[test]
fn test_closure_replaces_itself_through_its_by_ref_capture() {
    let out = compile_and_run(
        r#"<?php
function viaPlainRef(): void {
    $closure = null;
    $closure = static function () use (&$closure) {
        echo "plain in\n";
        $closure = static function () { echo "plain replaced\n"; };
    };
    $closure();
    $closure();
}
function viaArrayElement(): void {
    $arr = [];
    $closure = &$arr[];
    $closure = static function () use (&$closure) {
        echo "element in\n";
        $closure = static function () { echo "element replaced\n"; };
    };
    ($arr[0])();
    ($arr[0])();
}
viaPlainRef();
viaArrayElement();
"#,
    );
    assert_eq!(
        out,
        "plain in\nplain replaced\nelement in\nelement replaced\n"
    );
}

/// Verifies nested closure return records remain visible during array and callback specialization.
#[test]
fn test_nested_closure_return_scope_drives_array_and_callback_types() {
    let out = compile_and_run(
        r#"<?php
function relativeParts(string $endPath, string $startPath): string {
    $splitPath = static function ($path) {
        $result = [];
        foreach (explode('/', trim($path, '/')) as $segment) {
            if ('' !== $segment) {
                $result[] = $segment;
            }
        }
        return $result;
    };

    $startPathArr = $splitPath($startPath);
    $endPathArr = $splitPath($endPath);
    $index = 0;
    while (isset($startPathArr[$index]) && isset($endPathArr[$index]) && $startPathArr[$index] === $endPathArr[$index]) {
        ++$index;
    }
    if (1 === count($startPathArr) && '' === $startPathArr[0]) {
        $depth = 0;
    } else {
        $depth = count($startPathArr) - $index;
    }

    return str_repeat('../', $depth).implode('/', array_slice($endPathArr, $index));
}

function replacePart(bool $upper, string $part): string {
    $callback = $upper
        ? static fn ($matches) => strtoupper($matches[1])
        : static fn ($matches) => $matches[1];

    return preg_replace_callback('/([a-z]+)/', $callback, $part);
}

echo relativeParts('/a/b/c', '/a/d').'|'.replacePart(true, 'ab');
"#,
    );
    assert_eq!(out, "../b/c|AB");
}

/// A closure returning a property of an untyped parameter returns the property's value. The
/// closure signature typed that return syntactically as `int`, so the body converted the string
/// on its way out and `array_map(fn ($t) => $t->name, $objects)` produced a list of zeros.
#[test]
fn test_closure_returning_property_of_untyped_param_keeps_value() {
    let out = compile_and_run(
        r#"<?php
final class Named { public function __construct(public string $name) {} }
final class Holder { public ?Named $n = null; }
$f = fn ($t) => $t->name;
$g = function ($t) { return $t->name; };
echo $f(new Named('A')), $g(new Named('B')), "\n";
echo implode(',', array_map(fn ($t) => $t->name, [new Named('C'), new Named('D')])), "\n";
$h = new Holder();
$h->n = new Named('E');
$prop = 'name';
echo (fn ($m) => $m?->n?->name)($h), (fn ($t) => $t->$prop)(new Named('F')), "\n";
"#,
    );
    assert_eq!(out, "AB\nC,D\nEF\n");
}


/// A throw inside a first-class callable reaches the compiled `catch`, whatever the callable
/// names. The descriptor's invoker carries an exception boundary for the interpreter, and it used
/// to catch for every caller: an AOT `$f()` over `thrower(...)` returned NULL instead.
#[test]
fn test_first_class_callable_throw_reaches_compiled_catch() {
    let out = compile_and_run(
        r#"<?php
function thrower(string $n): string { throw new RuntimeException("fn:$n"); }
class A {
    public function pub(string $n): string { throw new RuntimeException("pub:$n"); }
    public static function stat(string $n): string { throw new RuntimeException("stat:$n"); }
}
function call(\Closure $f): mixed { return $f('x'); }
$a = new A();
foreach ([thrower(...), $a->pub(...), A::stat(...)] as $f) {
    try { call($f); echo "returned\n"; } catch (RuntimeException $e) { echo $e->getMessage(), "\n"; }
}
"#,
    );
    assert_eq!(out, "fn:x\npub:x\nstat:x\n");
}

/// `$this->m(...)` binds the method the RECEIVER's class resolves, like `$this->m()` does. It was
/// bound to the creating class's body, so a subclass override never ran through the callable.
#[test]
fn test_this_method_first_class_callable_is_late_bound() {
    let out = compile_and_run(
        r#"<?php
class Base {
    private ?\Closure $cb = null;
    protected function getEnv(string $n): string { return "base:$n"; }
    public function viaFcc(string $n): string { $this->cb ??= $this->getEnv(...); return ($this->cb)($n); }
}
class Child extends Base {
    protected function getEnv(string $n): string { return 'child(' . parent::getEnv($n) . ')'; }
}
echo (new Child())->viaFcc('a'), '|', (new Base())->viaFcc('b');
"#,
    );
    assert_eq!(out, "child(base:a)|base:b");
}

/// A single-expression closure returning a local its own expression assigns keeps the value's
/// type. Lowering typed that local syntactically as `int` and converted the string on the way
/// out: Laravel's `is_dir($directory = $this->basePath('.laravel')) ? $directory : ...` bootstrap
/// path closure returned "0".
#[test]
fn test_closure_returning_a_local_assigned_in_its_own_expression() {
    let out = compile_and_run(
        r#"<?php
function g($p) { return "/tmp/x/" . $p; }
$c = function () { return is_dir($d = g(".laravel")) ? $d : g("bootstrap"); };
var_dump($c());
$arrow = fn () => ($x = g("y")) ? $x : 0;
var_dump($arrow());
class A {
    function g($p) { return "/tmp/x/" . $p; }
    function c() { $c = function () { return is_dir($d = $this->g(".laravel")) ? $d : $this->g("bootstrap"); }; return $c(); }
}
var_dump((new A)->c());
"#,
    );
    assert_eq!(
        out,
        "string(16) \"/tmp/x/bootstrap\"\nstring(8) \"/tmp/x/y\"\nstring(16) \"/tmp/x/bootstrap\"\n"
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
    // The releases are inline `__rt_decref_any` calls, or one call into the shared
    // `_eir_invoker_finish` tail that performs them.
    let release = ["__rt_decref_any", "_eir_invoker_finish"]
        .iter()
        .filter_map(|needle| invoker.find(needle))
        .min()
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

/// Verifies an untyped arrow fn passed to a `callable` parameter accepts a string argument and
/// feeds it to a string builtin: an untyped parameter is `mixed`, never an `int` placeholder.
#[test]
fn test_untyped_arrow_fn_callable_param_string_arg() {
    let out = compile_and_run(
        r#"<?php
function apply_cb(callable $cb, string $key): mixed {
    return $cb($key);
}
echo apply_cb(fn($key) => strlen($key), "abcd"), "\n";
"#,
    );
    assert_eq!(out, "4\n");
}

/// Verifies an untyped closure stored in a variable can be called with a string and with an int.
#[test]
fn test_untyped_closure_variable_string_and_int_args() {
    let out = compile_and_run(
        r#"<?php
function apply_cb(callable $cb, string $key): mixed {
    return $cb($key);
}
$f = function ($k) { return strtoupper($k) . "!"; };
echo apply_cb($f, "ab"), "\n";
echo $f(12), "\n";
"#,
    );
    assert_eq!(out, "AB!\n12!\n");
}

/// Verifies array_map with an untyped arrow fn over strings keeps working with string builtins.
#[test]
fn test_untyped_arrow_fn_array_map_over_strings() {
    let out = compile_and_run(
        r#"<?php
echo implode(",", array_map(fn($s) => str_pad($s, 3, "*"), ["a", "bb"])), "\n";
"#,
    );
    assert_eq!(out, "a**,bb*\n");
}

/// Verifies a closure that only READS a by-reference capture sees what a sibling closure wrote.
///
/// Both closures share `$n`'s reference cell. The reader is built first, so unless the writer's
/// stores are visible when the reader's capture is typed, the reader keeps the capture-time `int`
/// representation and reinterprets the writer's boxed value: `null` read back as `int(0)` and an
/// array as its last element.
#[test]
fn test_by_ref_capture_reader_sees_sibling_closure_writes() {
    let out = compile_and_run(
        r#"<?php
function t() {
    $n = 1;
    $get = function () use (&$n) { return $n; };
    $set = function ($v) use (&$n) { $n = $v; };
    $set(null);
    var_dump($get());
    $set([1, 2]);
    echo json_encode($get()), "\n";
}
t();
"#,
    );
    assert_eq!(out, "NULL\n[1,2]\n");
}

/// A closure stored into a `mixed`-valued associative array is retained by its entry.
///
/// `['a' => $closure, 'c' => true]` and `$h['f'] = $closure` on a mixed hash wrote the
/// descriptor into a tag-10 entry without a retain, while the hash's deep free releases tag-10
/// entries. When the array died the closure was freed under its owner, and its block was reused
/// by the next closure allocated, so invoking the kept closure ran THAT closure's body. Symfony's
/// `OptionsResolver::setDefaults()` takes exactly this shape (lazy-default closures mixed with
/// scalars): `$closure($this, $value)` ran a normalizer instead of the lazy default.
/// Expected output is `php -n` 8.5's.
#[test]
fn test_closure_in_mixed_hash_survives_the_array() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Res {
    public array $lazy = [];
    public function setDefault(string $option, mixed $value): void {
        if ($value instanceof \Closure) {
            $this->lazy[$option] = $value;
        }
    }
}
function configure(Res $r): void {
    $a = static fn ($options) => 'A';
    foreach (['a' => $a, 'c' => true] as $option => $value) {
        $r->setDefault($option, $value);
    }
    $h = [];
    $h['x'] = 1;
    $h['f'] = $a;
    $r->setDefault('f', $h['f']);
}
$r = new Res();
configure($r);
$other = function ($o) { return "WRONG"; };
echo ($r->lazy["a"])(1), ($r->lazy["f"])(2), $other(3), "\n";
unset($r, $other);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "AAWRONG\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `$closure->call($object)` binds the object's class scope, not only `$this`.
///
/// Only `$this` was rebound, so the unscoped body raised "Cannot access private property" on
/// `$this->n` where php reads it. Expected output is `php -n` 8.5's.
#[test]
fn test_closure_call_binds_the_new_this_class_scope() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class CallCounter { private int $n = 41; private function bump(): int { return ++$this->n; } }
$peek = function () { return $this->n; };
echo $peek->call(new CallCounter()), "|";
$bump = function (int $by) { $this->n += $by; return $this->bump(); };
echo $bump->call(new CallCounter(), 5), "\n";
unset($peek, $bump);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "41|47\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A closure the interpreter made reports `Closure` from compiled `get_debug_type()`, also in a
/// body that owns no eval context, and reflection of a compiled closure whose body calls
/// `func_get_args()` does not list the hidden surplus-argument collector.
///
/// The eval closure is carried by a native object whose class id names `stdClass`, which is what
/// `get_debug_type()` printed; and `new ReflectionFunction($closure)` reported two parameters, the
/// second `$__elephc_func_args#gen`, and `isVariadic()`. Expected output is `php -n` 8.5's.
#[test]
fn test_closure_debug_type_and_reflection_hide_compiler_internals() {
    let out = compile_and_run(
        r#"<?php
$c = eval('return fn () => 9;');
function debugTypeOf(mixed $v): string { return get_debug_type($v); }
echo get_debug_type($c), "|", debugTypeOf($c), "|", get_class($c), "\n";
$h = function ($x) { return func_get_args(); };
$r = new ReflectionFunction($h);
echo $r->getNumberOfParameters(), ":", $r->isVariadic() ? "v" : "nv", ":";
foreach ($r->getParameters() as $p) { echo $p->getName(), ","; }
echo count($h(1, 2, 3)), "\n";
"#,
    );
    assert_eq!(out, "Closure|Closure|Closure\n1:nv:x,3\n");
}

/// A closure bound to another object reaches that object's members, private and protected ones
/// included, when the scope is given as the object's class NAME (`$k::class`, `get_class($k)`)
/// -- php treats the name and the object the same. The checker refused a method the enclosing
/// class does not declare, and the run time put a name-scoped closure in the global scope:
/// Symfony's debug commands call `$this->buildContainer()` through exactly this bind.
#[test]
fn test_closure_bound_with_a_class_name_scope_reaches_private_members() {
    let out = compile_and_run(
        r#"<?php
class Kernel {
    private int $n = 7;
    protected function buildContainer(): string { return "built"; }
}
class Cmd {
    public function run(Kernel $k): string {
        $a = \Closure::bind(function () { return $this->buildContainer() . ":" . $this->n; }, $k, $k::class);
        $b = \Closure::bind(function () { return $this->n; }, $k, get_class($k));
        return $a() . "|" . $b();
    }
}
echo (new Cmd)->run(new Kernel), "\n";
"#,
    );
    assert_eq!(out, "built:7|7\n");
}

/// In a closure rebound with `bindTo()` / `Closure::bind()`, `self::class` is the bound SCOPE and
/// `static::class` the class of the bound `$this` (the scope when there is none); an unbound
/// closure keeps its lexical class. The closure kept `Describer` for both. Verified against php 8.5.
#[test]
fn test_rebound_closure_self_and_static_class_follow_the_bound_scope() {
    let out = compile_and_run(
        r#"<?php
class P {}
class C extends P {}
class D extends C {}
class Describer {
    public function viaClosure(): \Closure {
        return fn () => [self::class, static::class, get_parent_class(self::class)];
    }
    public static function staticClosure(): \Closure {
        return static fn () => [self::class, static::class];
    }
}
$d = new Describer();
$f = $d->viaClosure();
echo json_encode($f()), "\n";
echo json_encode($f->bindTo(new D(), C::class)()), "\n";
echo json_encode($f->bindTo(new D(), D::class)()), "\n";
echo json_encode(\Closure::bind($d->viaClosure(), new C(), P::class)()[0]), "\n";
echo json_encode(\Closure::bind($d->viaClosure(), new C(), P::class)()[1]), "\n";
echo json_encode(\Closure::bind(Describer::staticClosure(), null, C::class)()), "\n";
echo json_encode(Describer::staticClosure()()), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "[\"Describer\",\"Describer\",\"\"]\n",
            "[\"C\",\"D\",\"P\"]\n",
            "[\"D\",\"D\",\"C\"]\n",
            "\"P\"\n",
            "\"C\"\n",
            "[\"C\",\"C\"]\n",
            "[\"Describer\",\"Describer\"]\n",
        )
    );
}

/// `parent::m()` in a closure written in a class with no parent is a non-virtual call to the
/// method of the bound scope's parent class, with `$this` forwarded -- Symfony's
/// `Descriptor::getEnvVarsFromDump()` writes `fn ($n) => parent::get($n)` and binds it to a
/// parameter bag. The backend refused it ("parent static method call outside class with
/// parent"). A scope with no parent raises php's `Error`. Verified against php 8.5.
#[test]
fn test_rebound_closure_parent_call_targets_the_bound_scopes_parent() {
    let out = compile_and_run(
        r#"<?php
class P { public function get(string $n): string { return "P:$n"; } }
class C extends P { public function get(string $n): string { return "C:$n"; } }
class D extends C { public function get(string $n): string { return "D:$n"; } }
class Describer {
    public function make(C $bag): \Closure {
        $f = fn (string $name) => parent::get($name);
        return $f->bindTo($bag, $bag::class);
    }
    public function parentCall(): \Closure {
        return fn (string $name) => parent::get($name);
    }
}
$d = new Describer();
echo $d->make(new C())('x'), "\n";
$g = $d->parentCall();
echo $g->bindTo(new D(), D::class)('a'), "\n";
echo $g->bindTo(new C(), C::class)('b'), "\n";
echo \Closure::bind($d->parentCall(), new D(), C::class)('c'), "\n";
try {
    echo $g->bindTo(new P(), P::class)('d'), "\n";
} catch (\Error $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "P:x\n",
            "C:a\n",
            "P:b\n",
            "P:c\n",
            "Error: Cannot access \"parent\" when current class scope has no parent\n",
        )
    );
}

/// A rebound `parent::m()` compiles when ANOTHER parent class's `m` needs more arguments.
///
/// The run-time ladder had an arm for every parent declaring `get`, including one whose `get`
/// requires two arguments; that arm made the backend refuse the closure (Symfony's
/// `Descriptor::getContainerEnvVars()` beside `AbstractAdapter::get()`). Expected output is
/// `php -n` 8.5's.
#[test]
fn test_rebound_parent_call_skips_parents_whose_method_cannot_take_the_arguments() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class P { public function get(string $n): string { return "P:$n"; } }
class C extends P { public function get(string $n): string { return "C:$n"; } }
class Q { public function get(string $key, callable $cb, ?float $beta = null): string { return "Q:$key"; } }
class R extends Q {}
class Describer {
    public function make(C $bag): \Closure {
        $f = fn (string $name) => parent::get($name);
        return $f->bindTo($bag, $bag::class);
    }
}
echo (new Describer())->make(new C())('x'), "\n";
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "P:x\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A run-time `Closure::bind` scope read from a GLOBAL variable is borrowed, not released.
///
/// `global $kernel` elsewhere gives the top-level `$kernel` global storage, and the bind's scope
/// argument `$kernel::class` (rewritten to the object) and `$cls` were released after the bind as
/// if they were temporaries: every bind dropped one reference of the kernel, which was freed
/// mid-loop. Symfony's `lint:container` binds `buildContainer()` to the kernel exactly this way and
/// then read freed memory as a service's method calls. Expected output is `php -n` 8.5's.
#[test]
fn test_runtime_bind_scope_read_from_a_global_is_not_released() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Kernel {
    private int $x = 1;
    public function __destruct() { echo "kernel gone\n"; }
    protected function build(): int { return $this->x + 1; }
}
function peek(): string { global $kernel; return get_class($kernel); }
$kernel = new Kernel();
for ($i = 0; $i < 3; $i++) {
    $f = \Closure::bind(function () { return $this->build(); }, $kernel, $kernel::class);
    echo $f(), " ";
    $cls = $kernel::class;
    $g = \Closure::bind(function () { return $this->x; }, $kernel, $cls);
    echo $g(), " ";
}
echo peek(), "\n";
unset($f, $g);
echo "before unset\n";
unset($kernel);
echo "end\n";
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "2 1 2 1 2 1 Kernel\nbefore unset\nkernel gone\nend\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An arrow function that defines its own recursive closure compiles and runs.
///
/// An arrow function captures every outer variable its body names, and php skips the ones that do
/// not exist. The body here assigns `$f` itself (`$f = function () use (&$f) {...}`), so `$f` is a
/// local, not a capture; the checker refused it as "Undefined variable in use(): $f". This is the
/// shape of Laravel's `Vite::makePreloadTags()`. Expected output is real php 8.5 output.
#[test]
fn test_arrow_function_defining_a_recursive_closure_is_not_an_undefined_capture() {
    let out = compile_and_run(
        r#"<?php
$factorial = fn ($n) => ($f = function ($k) use (&$f) { return $k <= 1 ? 1 : $k * $f($k - 1); })($n);
echo $factorial(5), "\n";
$items = [3, 4];
$sums = array_map(fn ($n) => ($g = function ($k) use (&$g) { return $k <= 0 ? 0 : $k + $g($k - 1); })($n), $items);
echo implode(",", $sums), "\n";
"#,
    );
    assert_eq!(out, "120\n6,10\n");
}

/// Verifies a closure can capture a function `static` BY REFERENCE: the closure writes the
/// persistent storage, and the next call of the function sees it. Symfony's
/// `HttpClientTrait::normalizeBody()` has this shape (`static $cookie;` captured by a recursive
/// `array_walk_recursive` caster); the backend refused it, which sent `CurlHttpClient` to the
/// interpreter and forced a second compile of the Symfony demo. Expected output is php 8.5's.
#[test]
fn test_closure_captures_a_function_static_by_reference() {
    let out = compile_and_run(
        r#"<?php
function normalize(array $body): array {
    static $cookie;
    $streams = [];
    array_walk_recursive($body, $caster = static function (&$v) use (&$caster, &$streams, &$cookie) {
        if (is_int($v)) {
            $cookie = ($cookie ?? 0) + 1;
            $streams['s' . $cookie] = $v;
            $v = 's' . $cookie;
        }
    });
    $caster = null;
    return [$body, $streams, $cookie];
}
function tick(): string {
    static $n = null;
    $f = static function () use (&$n) { $n = ($n ?? 0) + 1; };
    $f();
    $f();
    return (string) $n;
}
echo json_encode(normalize(['a' => 1, 'b' => ['c' => 2, 'd' => 'x']])), "\n";
echo json_encode(normalize([5])), "\n";
echo tick(), ",", tick(), "\n";
"#,
    );
    assert_eq!(
        out,
        "[{\"a\":\"s1\",\"b\":{\"c\":\"s2\",\"d\":\"x\"}},{\"s1\":1,\"s2\":2},2]\n[[\"s3\"],{\"s3\":5},3]\n2,4\n"
    );
}

/// A throw out of a closure called through a runtime-bound descriptor unwinds to the caller's
/// `catch`. The shared invoker entry once wrote its exception-boundary record through `main`'s
/// frame-base register (x28) instead of the invoker's own frame, so the closure's frame record
/// linked to a garbage predecessor and the unwinder jumped into it.
#[test]
fn test_throw_out_of_a_bound_closure_reaches_the_callers_catch() {
    let out = compile_and_run(
        r#"<?php
class Vault { public string $code = "open"; }
$peek = function () { throw new Exception("x"); };
$bound = Closure::bind($peek, new Vault());
try { $bound(); } catch (Exception $error) { echo "caught|"; }
$again = Closure::bind($peek, new Vault(), Vault::class);
try { $again(); } catch (Exception $error) { echo "caught again"; }
"#,
    );
    assert_eq!(out, "caught|caught again");
}

/// A closure bound to a class scope at compile time still reads that class's private property when
/// it also carries the hidden runtime-scope capture (it reads `$this->code` outside any class), and
/// a plain bind keeps denying it.
#[test]
fn test_compile_time_scope_variant_keeps_the_hidden_scope_capture() {
    let out = compile_and_run(
        r#"<?php
class Vault { private string $code = "open"; }
$peek = function () { return $this->code; };
$scoped = Closure::bind($peek, new Vault(), Vault::class);
echo $scoped(), "|";
$rebound = $scoped->bindTo(new Vault(), Vault::class);
echo $rebound(), "|";
try { echo Closure::bind($peek, new Vault())(); } catch (Error $error) { echo "denied"; }
"#,
    );
    assert_eq!(out, "open|open|denied");
}

/// `f()->bindTo($newThis, $scope)` destroys its temporaries in php's order: the scope argument
/// before the closure that carries the old `$this`.
#[test]
fn test_bind_to_on_a_call_result_retires_the_scope_before_the_source_closure() {
    let out = compile_and_run(
        r#"<?php
class V {
    public function __construct(public string $name) {}
    public function reader(): callable { return function () { return $this->name; }; }
    public function __destruct() { echo $this->name, "|"; }
}
function reader(): callable { return (new V("old"))->reader(); }
$x = reader()->bindTo(new V("n"), new V("s"));
echo "stmt|";
"#,
    );
    assert_eq!(out, "s|old|stmt|n|");
}
