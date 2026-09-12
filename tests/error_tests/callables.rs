//! Purpose:
//! Integration or regression tests for diagnostic coverage of callables, including call user func wrong args, function exists wrong args, and call non callable variable.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// An unsupported fourth replacement argument stays rejected through callable syntax.
#[test]
fn test_error_capped_string_replace_callable_rejects_fourth_argument() {
    for name in ["str_replace", "str_ireplace"] {
        expect_error(
            &format!("<?php {name}('a', 'b', 'aAa', 0);"),
            "3 arguments",
        );
        expect_error(
            &format!("<?php $callback = {name}(...); $callback('a', 'b', 'aAa', 0);"),
            "3 arguments",
        );
    }
}

/// Final-pass callable argument errors remain fatal even beside calls whose metadata stabilized.
#[test]
fn test_error_callable_property_metadata_does_not_hide_invalid_arguments() {
    for source in [
        r#"<?php
class CallableParameterGuard {
    public $callback;
    public function install(): void { $this->callback = static fn(): int => 1; }
    public function accept(callable $callback): void {}
}
$guard = new CallableParameterGuard();
$guard->install();
$guard->accept($guard->callback);
$guard->accept(null);
"#,
        r#"<?php
class CallableParameterGuard {
    public $callback;
    public function accept(callable $callback): void {}
}
$guard = new CallableParameterGuard();
$guard->accept($guard->callback);
"#,
    ] {
        expect_error(source, "parameter $callback expects Callable, got Void");
    }
}

/// Declaration defaults do not permit explicit literal arguments to bind by reference.
#[test]
fn test_error_named_reference_defaults_still_reject_supplied_literals() {
    for source in [
        "<?php function f(int &$out = 7, int $value = 0): void {} f(value: 1, out: 7);",
        "<?php function f(array &$out = [10], int $value = 0): void {} f(value: 1, out: [10]);",
        "<?php function f(array &$out = [10], int $value = 0): void {} $f = f(...); $f(value: 1, out: [10]);",
        "<?php class C { public function f(array &$out = [10], int $value = 0): void {} } $c = new C(); $c->f(value: 1, out: [10]);",
        "<?php class C { public static function f(array &$out = [10], int $value = 0): void {} } C::f(value: 1, out: [10]);",
        "<?php class C { public function __construct(array &$out = [10], int $value = 0) {} } new C(value: 1, out: [10]);",
    ] {
        expect_error(source, "parameter $out must be passed a variable");
    }
}

/// Runtime unpack support does not relax the arity of concrete introspection calls.
#[test]
fn test_error_core_introspection_concrete_arity() {
    for name in ["get_class_vars", "get_class_methods"] {
        expect_error(
            &format!("<?php {name}();"),
            &format!("{name}() takes exactly 1 argument"),
        );
        expect_error(
            &format!("<?php {name}('stdClass', 'stdClass');"),
            &format!("{name}() takes exactly 1 argument"),
        );
    }
}

/// Shared introspection validators still reject concrete invalid positional and named values.
#[test]
fn test_error_core_introspection_concrete_argument_types() {
    for (name, parameter, expected) in [
        ("get_class_vars", "class", "get_class_vars() argument must be a string in AOT mode"),
        ("get_class_methods", "object_or_class", "get_class_methods() argument must be an object or string in AOT mode"),
    ] {
        expect_error(&format!("<?php {name}(false);"), expected);
        expect_error(&format!("<?php {name}({parameter}: false);"), expected);
    }
}

/// Verifies that error call user func wrong args.
#[test]
fn test_error_call_user_func_wrong_args() {
    // Verifies `call_user_func()` with no arguments produces a diagnostic about
    // requiring at least 1 argument.
    expect_error(
        r#"<?php call_user_func();"#,
        "call_user_func() takes at least 1 argument",
    );
}

/// Verifies that error function exists wrong args.
#[test]
fn test_error_function_exists_wrong_args() {
    // Verifies `function_exists()` with no arguments produces a diagnostic about
    // requiring exactly 1 argument.
    expect_error(
        r#"<?php function_exists();"#,
        "function_exists() takes exactly 1 argument",
    );
}

/// Verifies that error class exists requires literal name.
#[test]
fn test_error_class_exists_requires_literal_name() {
    // Verifies `class_exists()` with a runtime variable as the first argument
    // produces a diagnostic because AOT mode requires a string literal.
    expect_error(
        r#"<?php $name = "DateTime"; class_exists($name);"#,
        "class_exists() first argument must be a string literal in AOT mode",
    );
}

// NOTE: class_exists() (and the other class-like existence probes) accept a
// dynamic autoload flag: it never contributes an AOT autoload demand and
// existence still folds from the literal class name. Only the class-relation
// builtins (class_implements/class_parents/class_uses) keep the literal
// autoload requirement, covered below.

/// Verifies that error interface exists wrong args.
#[test]
fn test_error_interface_exists_wrong_args() {
    // Verifies `interface_exists()` with no arguments produces a diagnostic about
    // requiring 1 or 2 arguments.
    expect_error(
        r#"<?php interface_exists();"#,
        "interface_exists() takes 1 or 2 arguments",
    );
}

/// Verifies that error trait exists wrong args.
#[test]
fn test_error_trait_exists_wrong_args() {
    // Verifies `trait_exists()` with no arguments produces a diagnostic about
    // requiring 1 or 2 arguments.
    expect_error(
        r#"<?php trait_exists();"#,
        "trait_exists() takes 1 or 2 arguments",
    );
}

/// Verifies that error enum exists wrong args.
#[test]
fn test_error_enum_exists_wrong_args() {
    // Verifies `enum_exists()` with no arguments produces a diagnostic about
    // requiring 1 or 2 arguments.
    expect_error(
        r#"<?php enum_exists();"#,
        "enum_exists() takes 1 or 2 arguments",
    );
}

/// Verifies that error class implements wrong args.
#[test]
fn test_error_class_implements_wrong_args() {
    expect_error(
        r#"<?php class_implements();"#,
        "class_implements() takes 1 or 2 arguments",
    );
}

/// Verifies that error class implements requires literal or object.
#[test]
fn test_error_class_implements_requires_literal_or_object() {
    expect_error(
        r#"<?php $name = "DateTime"; class_implements($name);"#,
        "class_implements() first argument must be an object or string literal in AOT mode",
    );
}

/// Verifies that error class parents requires literal autoload flag.
#[test]
fn test_error_class_parents_requires_literal_autoload_flag() {
    expect_error(
        r#"<?php $autoload = true; class_parents("DateTime", $autoload);"#,
        "class_parents() autoload argument must be a literal bool or int in AOT mode",
    );
}

/// Verifies that error class uses wrong args.
#[test]
fn test_error_class_uses_wrong_args() {
    expect_error(
        r#"<?php class_uses("DateTime", true, false);"#,
        "class_uses() takes 1 or 2 arguments",
    );
}

/// Verifies that error get class wrong args.
#[test]
fn test_error_get_class_wrong_args() {
    // Verifies `get_class()` with a second argument produces a diagnostic about
    // accepting at most 1 argument.
    expect_error(
        r#"<?php class Box {} $box = new Box(); get_class($box, $box);"#,
        "get_class() takes at most 1 argument",
    );
}

/// Verifies that error get parent class wrong args.
#[test]
fn test_error_get_parent_class_wrong_args() {
    // Verifies `get_parent_class()` with a second argument produces a diagnostic
    // about accepting at most 1 argument.
    expect_error(
        r#"<?php class Box {} $box = new Box(); get_parent_class($box, $box);"#,
        "get_parent_class() takes at most 1 argument",
    );
}

/// Verifies that error is subclass of wrong args.
#[test]
fn test_error_is_subclass_of_wrong_args() {
    // Verifies `is_subclass_of()` with only 1 argument produces a diagnostic
    // about requiring 2 or 3 arguments.
    expect_error(
        r#"<?php is_subclass_of("Child");"#,
        "is_subclass_of() takes 2 or 3 arguments",
    );
}

/// Verifies that error is a wrong args.
#[test]
fn test_error_is_a_wrong_args() {
    // Verifies `is_a()` with only 1 argument produces a diagnostic about
    // requiring 2 or 3 arguments.
    expect_error(
        r#"<?php is_a("Child");"#,
        "is_a() takes 2 or 3 arguments",
    );
}

/// Verifies that error get declared classes wrong args.
#[test]
fn test_error_get_declared_classes_wrong_args() {
    // Verifies `get_declared_classes()` with an extra argument produces a
    // diagnostic about accepting no arguments.
    expect_error(
        r#"<?php get_declared_classes("extra");"#,
        "get_declared_classes() takes no arguments",
    );
}

/// Verifies that error get declared interfaces wrong args.
#[test]
fn test_error_get_declared_interfaces_wrong_args() {
    // Verifies `get_declared_interfaces()` with an extra argument produces a
    // diagnostic about accepting no arguments.
    expect_error(
        r#"<?php get_declared_interfaces("extra");"#,
        "get_declared_interfaces() takes no arguments",
    );
}

/// Verifies that error get declared traits wrong args.
#[test]
fn test_error_get_declared_traits_wrong_args() {
    // Verifies `get_declared_traits()` with an extra argument produces a
    // diagnostic about accepting no arguments.
    expect_error(
        r#"<?php get_declared_traits("extra");"#,
        "get_declared_traits() takes no arguments",
    );
}

/// Verifies that error class alias rejects runtime call shape.
#[test]
fn test_error_class_alias_rejects_runtime_call_shape() {
    // Verifies `class_alias()` with a runtime variable as the second argument
    // produces a diagnostic because only top-level statements with literal
    // class names are supported in AOT mode.
    expect_error(
        r#"<?php class Original {} $alias = "Alias"; class_alias("Original", $alias);"#,
        "class_alias() is only supported as a top-level statement with literal class names",
    );
}

// --- Closure / arrow function errors ---

/// Verifies that error call non callable variable.
#[test]
fn test_error_call_non_callable_variable() {
    // Verifies invoking a non-callable variable (integer) produces a "not a callable"
    // diagnostic at runtime.
    expect_error(r#"<?php $x = 5; $x(1);"#, "not a callable");
}

/// Boxed runtime dispatch still rejects unpacking after an explicit named argument.
#[test]
fn test_error_boxed_direct_callable_spread_after_named_argument() {
    for call in ["$callback(value: 1, ...[2]);", "$callbacks[0](value: 1, ...[2]);"] {
        expect_error(
            &format!("<?php function targets(): array {{ return [fn(int $value): int => $value]; }} $callbacks = targets(); $callback = $callbacks[0]; {call}"),
            "cannot use argument unpacking after named arguments",
        );
    }
}

/// Verifies that error call user func ref param requires variable.
#[test]
fn test_error_call_user_func_ref_param_requires_variable() {
    // Verifies `call_user_func()` with a closure that has a by-reference
    // parameter and a non-variable argument produces a diagnostic requiring
    // a variable to be passed.
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } $f = bump(...); call_user_func($f, 1);",
        "parameter $n must be passed a variable",
    );
}

/// Verifies that error call user func string literal ref param requires variable.
#[test]
fn test_error_call_user_func_string_literal_ref_param_requires_variable() {
    // Verifies `call_user_func()` with a named function string and a by-reference
    // parameter passed a non-variable argument produces a diagnostic requiring
    // a variable to be passed.
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } call_user_func(\"bump\", 1);",
        "parameter $n must be passed a variable",
    );
}

/// Verifies that error case insensitive function string introspection keeps callback checks.
#[test]
fn test_error_case_insensitive_function_string_introspection_keeps_callback_checks() {
    // Verifies that case-insensitive function string introspection via
    // `function_exists("BUMP")` and `is_callable("BUMP")` still enforces
    // by-reference parameter semantics when `call_user_func("BUMP", ...)` is
    // subsequently invoked.
    expect_error(
        "<?php function Bump(&$n) { $n = $n + 1; } if (function_exists(\"BUMP\") && is_callable(\"BUMP\")) { call_user_func(\"BUMP\", 1); }",
        "parameter $n must be passed a variable",
    );
}

/// Verifies that error closure return type rejects mismatch.
#[test]
fn test_error_closure_return_type_rejects_mismatch() {
    // Verifies a closure with an explicit return type that returns a mismatched
    // type produces a diagnostic showing the expected and actual types.
    expect_error(
        "<?php $f = function(): string { return 1; };",
        "Closure return type expects Str, got Int",
    );
}

/// Verifies that error arrow return type rejects mismatch.
#[test]
fn test_error_arrow_return_type_rejects_mismatch() {
    // Verifies an arrow function with an explicit return type that returns a
    // mismatched type produces a diagnostic showing the expected and actual types.
    expect_error(
        "<?php $f = fn(): int => \"nope\";",
        "Closure return type expects Int, got Str",
    );
}

/// Verifies that error closure return type requires return value.
#[test]
fn test_error_closure_return_type_requires_return_value() {
    // Verifies a closure with an explicit return type and an empty body (no return)
    // produces a diagnostic about every path needing to return a value.
    expect_error(
        "<?php $f = function(): int { };",
        "Closure must return a value on every path",
    );
}

/// Verifies that error closure return type rejects partial fallthrough.
#[test]
fn test_error_closure_return_type_rejects_partial_fallthrough() {
    // Verifies a closure with an explicit return type where only some branches
    // return a value (missing return in else branch) produces a diagnostic
    // about every path needing to return a value.
    expect_error(
        "<?php $f = function(bool $ok): int { if ($ok) { return 1; } };",
        "Closure must return a value on every path",
    );
}

/// Verifies that error closure return type rejects bare return.
#[test]
fn test_error_closure_return_type_rejects_bare_return() {
    // Verifies a closure with `mixed` return type and a bare `return;` (no value)
    // produces a diagnostic about needing to return a value of the specified type.
    expect_error(
        "<?php $f = function(): mixed { return; };",
        "Closure return type must return a value of type",
    );
}

/// Verifies that error closure void return type rejects value.
#[test]
fn test_error_closure_void_return_type_rejects_value() {
    // Verifies a closure with `void` return type that returns a value produces
    // a diagnostic about not returning a value.
    expect_error(
        "<?php $f = function(): void { return 1; };",
        "Closure return type must not return a value",
    );
}

/// Verifies `new FiberError(...)` is refused, as reference PHP refuses it.
///
/// PHP reserves the class for internal use and gives it no user-callable constructor, so
/// `new FiberError("boom")` raises an `Error` there; here it produced a working object. The
/// engine still RAISES a `FiberError` of its own and user code still catches it by name —
/// `test_fiber_error_is_still_raised_and_catchable` is the other half, and without it a guard
/// that simply removed the class would look correct here.
#[test]
fn test_error_fiber_error_is_reserved_for_internal_use() {
    expect_error(
        r#"<?php $e = new FiberError("boom");"#,
        "The \"FiberError\" class is reserved for internal use and cannot be manually instantiated",
    );
}

/// Verifies that error fiber callback rejects too many start args.
#[test]
fn test_error_fiber_callback_rejects_too_many_start_args() {
    // Verifies a `Fiber` with a callback accepting 8 start arguments produces
    // a diagnostic because Fibers support at most 7 start arguments.
    expect_error(
        "<?php $fiber = new Fiber(function($a, $b, $c, $d, $e, $f, $g, $h): void {});",
        "Fiber callbacks support at most 7 start arguments, got 8",
    );
}

/// Verifies that error fiber callback rejects by ref start arg.
#[test]
fn test_error_fiber_callback_rejects_by_ref_start_arg() {
    // Verifies a `Fiber` with a callback that receives a start argument
    // by reference produces a diagnostic because by-reference start args
    // are not supported.
    expect_error(
        "<?php $fiber = new Fiber(function(&$value): void {});",
        "Fiber callbacks cannot receive start arguments by reference",
    );
}

// --- PHP 8.5 pipe operator ---

/// Verifies that error pipe rhs integer not callable.
#[test]
fn test_error_pipe_rhs_int_not_callable() {
    // Verifies the pipe operator (`|>`) with a plain integer on the right-hand
    // side produces a "must be a callable" diagnostic.
    expect_error(
        "<?php $r = 5 |> 42;",
        "must be a callable",
    );
}

/// Verifies that error pipe rhs string literal not callable.
#[test]
fn test_error_pipe_rhs_string_literal_not_callable() {
    // Verifies the pipe operator (`|>`) with a bare string literal on the RHS
    // produces a "must be a callable" diagnostic because string literals are
    // treated as `Str`, not `Callable`, at compile time.
    expect_error(
        "<?php $r = 5 |> \"strlen\";",
        "must be a callable",
    );
}

/// Verifies that error pipe rejects by ref parameter.
#[test]
fn test_error_pipe_rejects_by_ref_parameter() {
    // Verifies the pipe operator (`|>`) with a function that has by-reference
    // parameters produces a diagnostic because by-reference parameters are not
    // supported with the pipe operator.
    expect_error(
        "<?php function bump(int &$n): int { return ++$n; } $r = 1 |> bump(...);",
        "by-reference parameters",
    );
}

/// Verifies that error pipe target requires more than one required arg.
#[test]
fn test_error_pipe_target_requires_more_than_one_required_arg() {
    // Verifies the pipe operator (`|>`) with a callable that requires more than
    // one argument and is called without sufficient arguments produces a diagnostic
    // showing the expected vs received argument count.
    expect_error(
        "<?php function pair(int $a, int $b): int { return $a + $b; } $r = 1 |> pair(...);",
        "expects 2 arguments, got 1",
    );
}

/// Verifies that error pipe closure literal requires two args.
#[test]
fn test_error_pipe_closure_literal_requires_two_args() {
    // Verifies the pipe operator (`|>`) with a closure literal that expects two
    // arguments but receives only one (via the pipe's left-hand side) produces
    // a diagnostic showing the expected vs received argument count.
    expect_error(
        "<?php $r = 1 |> (function(int $a, int $b): int { return $a + $b; });",
        "pipe target expects 2 arguments, got 1",
    );
}

/// Verifies that error pipe closure literal rejects by ref parameter.
#[test]
fn test_error_pipe_closure_literal_rejects_by_ref_parameter() {
    // Verifies the pipe operator (`|>`) with a closure literal containing a
    // by-reference parameter produces a diagnostic because by-reference
    // parameters are not supported with the pipe operator.
    expect_error(
        "<?php $r = 1 |> (function(&$n): int { return $n; });",
        "Pipe operator does not support by-reference parameters",
    );
}

/// Verifies that error pipe closure literal typed parameter mismatch.
#[test]
fn test_error_pipe_closure_literal_typed_parameter_mismatch() {
    // Verifies the pipe operator (`|>`) with a closure literal that has a typed
    // parameter where the piped value's type does not match produces a diagnostic
    // showing the expected vs actual parameter type.
    expect_error(
        r#"<?php $r = "nope" |> (function(int $n): int { $copy = $n; return $copy; });"#,
        "pipe target parameter $n expects Int, got Str",
    );
}

// --- Argument introspection (`func_num_args` / `func_get_args` / `func_get_arg`) ---

/// Verifies that calling an argument-introspection construct outside any function reports
/// PHP's "must be called from a function context" error instead of an undefined function.
#[test]
fn test_error_func_num_args_outside_function() {
    expect_error(
        "<?php echo func_num_args();",
        "func_num_args() must be called from a function context",
    );
}

/// Verifies php-src's rule that these constructs cannot be called dynamically, here through
/// first-class callable syntax.
#[test]
fn test_error_func_num_args_first_class_callable() {
    expect_error(
        "<?php function f() { $g = func_num_args(...); return $g(); } echo f(1);",
        "Cannot call func_num_args() dynamically",
    );
}

/// Verifies the arity check: `func_num_args()` takes no arguments.
#[test]
fn test_error_func_num_args_rejects_arguments() {
    expect_error(
        "<?php function f() { return func_num_args(1); } echo f();",
        "func_num_args() expects 0 arguments, got 1",
    );
}

/// Verifies the arity check: `func_get_arg()` requires its `$position` argument.
#[test]
fn test_error_func_get_arg_requires_position() {
    expect_error(
        "<?php function f() { return func_get_arg(); } echo f(1);",
        "func_get_arg() expects 1 arguments, got 0",
    );
}

/// Verifies that surplus *named* arguments stay rejected for a function that only carries
/// the hidden argument-collection parameter: PHP accepts extra positional arguments there
/// but still rejects an unknown named one.
#[test]
fn test_error_surplus_named_argument_to_introspecting_function() {
    expect_error(
        "<?php function f($a) { return func_num_args(); } echo f(1, c: 3);",
        "has no parameter $c",
    );
}

/// Verifies that a method using the constructs while implementing an interface method is
/// rejected with a targeted message: the inherited signature has no slot for the collected
/// surplus arguments.
#[test]
fn test_error_func_get_args_in_interface_implementation() {
    expect_error(
        "<?php interface I { public function q(); } class C implements I { public function q() { return func_num_args(); } }",
        "the inherited signature cannot be widened to collect surplus arguments",
    );
}

/// Verifies a callable string that names nothing is rejected at compile time with the reason
/// spelled out. PHP throws `TypeError` when the call runs; elephc resolves callables
/// statically, so the same program cannot be built.
#[test]
fn test_error_callable_parameter_rejects_unknown_name_string() {
    expect_error(
        "<?php function apply(callable $f, string $s) { return $f($s); } echo apply(\"nosuchfn\", \"a\");",
        "Undefined function for first-class callable: nosuchfn",
    );
}

/// An eval barrier permits only the runtime function-table form of a literal CUF callback.
#[test]
fn test_eval_barrier_allows_a_runtime_declared_call_user_func_name() {
    for source in [
        "<?php eval('function dyn_eval_cuf($value) { return $value + 1; }'); echo call_user_func('dyn_eval_cuf', 4);",
        r#"<?php eval('namespace EvalInnerNs; function dyn_eval_inner_ns() { return 7; }'); echo call_user_func("EvalInnerNs\\dyn_eval_inner_ns");"#,
    ] {
        expect_no_error(source);
    }
}

/// Without an eval barrier, an unknown literal CUF callback remains a compile-time error.
#[test]
fn test_error_call_user_func_rejects_an_unknown_literal_without_eval() {
    expect_error(
        "<?php echo call_user_func('never_declared', 1);",
        "Undefined function for first-class callable: never_declared",
    );
}

/// Verifies a callable string that is only known at run time is rejected with a named
/// diagnostic instead of being bound to storage the callee could not invoke.
#[test]
fn test_error_callable_parameter_rejects_runtime_string() {
    expect_error(
        "<?php function apply(callable $f, string $s) { return $f($s); } $n = $argc > 0 ? \"strtoupper\" : \"strtolower\"; echo apply($n, \"a\");",
        "a callable string must be a compile-time constant here",
    );
}

/// Verifies that a callable ARRAY local satisfies a declared `callable` parameter.
///
/// `[$object, "method"]` keeps ordinary two-element array storage, so its inferred type is not
/// `Callable`; the only record that it names a method is the target metadata captured at the
/// assignment. Requiring a `Callable` storage type here rejected a PHP-valid program the callee
/// then invokes through exactly that metadata.
#[test]
fn test_callable_parameter_accepts_a_callable_array_local() {
    expect_no_error(
        "<?php class Joiner { public function join(string $left, string $right): string { return $left . $right; } } function apply(callable $f, string $a, string $b): string { return $f($a, $b); } $callback = [new Joiner(), \"join\"]; echo apply($callback, \"x\", \"y\");",
    );
}

/// Verifies the narrowness of that acceptance: an ordinary array with no callable-target
/// metadata is still rejected, so array storage did not become universally callable.
#[test]
fn test_error_callable_parameter_rejects_a_plain_array() {
    expect_error(
        "<?php function apply(callable $f): mixed { return $f(); } $values = [1, 2]; echo apply($values);",
        "expects Callable",
    );
}

/// Verifies that a two-element array whose first entry is not an object stays rejected: the
/// shape alone must not stand in for a resolved target.
#[test]
fn test_error_callable_parameter_rejects_an_unresolved_pair() {
    expect_error(
        "<?php function apply(callable $f): mixed { return $f(); } $pair = [1, \"join\"]; echo apply($pair);",
        "expects Callable",
    );
}

/// Callable-array facts from different reachable branches must not leak past the join.
#[test]
fn test_error_callable_array_target_is_cleared_when_branches_disagree() {
    expect_error(
        "<?php class BranchCallable { public static function left(): int { return 1; } public static function right(): int { return 2; } } function consume(callable $callback): int { return $callback(); } function choose(int $value): int { if ($value > 0) { $callback = [BranchCallable::class, 'left']; } else { $callback = [BranchCallable::class, 'right']; } return consume($callback); } echo choose($argc);",
        "expects Callable",
    );
}

/// An elseif path starts from the prior condition's false-path facts, not its true body.
#[test]
fn test_error_elseif_does_not_inherit_callable_target_from_prior_body() {
    expect_error(
        "<?php class ElseifCallable { public static function base(): int { return 0; } public static function changed(): int { return 1; } } function consume(callable $callback): int { return $callback(); } function choose(int $value): int { $callback = [ElseifCallable::class, 'base']; if ($value === 1) { $callback = [ElseifCallable::class, 'changed']; } elseif ($value === 2) { $marker = 2; } else { $marker = 3; } return consume($callback); } echo choose($argc);",
        "expects Callable",
    );
}

/// Identical static callable targets remain proven across every reachable branch.
#[test]
fn test_callable_array_target_is_retained_when_branches_agree() {
    expect_no_error(
        "<?php class BranchCallable { public static function hit(): int { return 1; } } function consume(callable $callback): int { return $callback(); } function choose(int $value): int { if ($value > 0) { $callback = [BranchCallable::class, 'hit']; } else { $callback = [BranchCallable::class, 'hit']; } return consume($callback); } echo choose($argc);",
    );
}

/// An instance target captured before a split remains valid when neither arm rewrites it.
#[test]
fn test_preexisting_instance_callable_array_survives_an_untouched_join() {
    expect_no_error(
        "<?php class BranchInstanceCallable { public function hit(): int { return 1; } } function consume(callable $callback): int { return $callback(); } function choose(int $value): int { $callback = [new BranchInstanceCallable(), 'hit']; if ($value > 0) { $marker = 1; } else { $marker = 2; } return consume($callback) + $marker; } echo choose($argc);",
    );
}

/// Both arms may copy the same pre-captured receiver without creating separate captures.
#[test]
fn test_branch_copies_of_one_instance_callable_array_can_join() {
    expect_no_error(
        "<?php class BranchInstanceCallable { public function hit(): int { return 1; } } function consume(callable $callback): int { return $callback(); } function choose(int $value): int { $source = [new BranchInstanceCallable(), 'hit']; if ($value > 0) { $callback = $source; } else { $callback = $source; } return consume($callback); } echo choose($argc);",
    );
}

/// Equal instance-target syntax in separate arms does not identify one captured receiver.
#[test]
fn test_error_branch_local_instance_callable_arrays_do_not_merge_by_syntax() {
    expect_error(
        "<?php class BranchInstanceCallable { public function hit(): int { return 1; } } function consume(callable $callback): int { return $callback(); } function choose(int $value): int { $receiver = new BranchInstanceCallable(); if ($value > 0) { $receiver = new BranchInstanceCallable(); $callback = [$receiver, 'hit']; } else { $receiver = new BranchInstanceCallable(); $callback = [$receiver, 'hit']; } return consume($callback); } echo choose($argc);",
        "expects Callable",
    );
}

/// Dynamic unpacking cannot turn nested PHP callable arrays into descriptor values.
#[test]
fn test_error_callable_parameter_spread_rejects_callable_array_elements() {
    expect_error(
        "<?php class SpreadCallable { public static function hit(): int { return 1; } } function consume(callable $callback): int { return $callback(); } $callbacks = [[SpreadCallable::class, 'hit']]; echo consume(...$callbacks);",
        "must contain Callable descriptors",
    );
}

/// Dynamic unpacking keeps working when the source already stores descriptor values.
#[test]
fn test_callable_parameter_spread_accepts_descriptor_elements() {
    expect_no_error(
        "<?php class SpreadCallable { public static function hit(): int { return 1; } } function consume(callable $callback): int { return $callback(); } $callbacks = [SpreadCallable::hit(...)]; echo consume(...$callbacks);",
    );
}

/// A direct Mixed argument is not a descriptor projection and stays outside Callable unboxing.
#[test]
fn test_error_direct_mixed_argument_does_not_gain_descriptor_unboxing() {
    for source in [
        "<?php function consume(callable $callback): int { return $callback(); } function forward(mixed $value): int { return consume($value); } echo forward(null);",
        "<?php function consume(callable $callback): int { return $callback(); } function forward(callable $dispatch, mixed $value): int { return $dispatch($value); } echo forward(consume(...), null);",
    ] {
        expect_error(source, "parameter $callback expects Callable, got Mixed");
    }
}

/// A spread cannot synthesize the lvalue identity required by a by-reference Callable parameter.
#[test]
fn test_error_callable_parameter_by_ref_spread_stays_rejected() {
    expect_error(
        "<?php class ByRefSpreadTarget { public static function hit(): int { return 1; } } function consume(callable &$callback, int $count): int { return $callback() + $count; } function forward(array $callbacks): int { return consume(...$callbacks, count: 1); } echo forward([ByRefSpreadTarget::hit(...)]);",
        "cannot be invoked with spread arguments when it has pass-by-reference parameters",
    );
}

/// Descriptor calls may walk Traversable spreads for untyped string and callable-array targets.
#[test]
fn test_untyped_call_user_func_targets_accept_traversable_spreads() {
    expect_no_error(
        "<?php class DescriptorValues implements IteratorAggregate { public function getIterator(): Traversable { yield 5; } } function descriptorFunction($value): int { return $value; } class DescriptorMethods { public function instanceValue($value): int { return $value; } public static function staticValue($value): int { return $value; } } $values = new DescriptorValues(); $methods = new DescriptorMethods(); echo call_user_func('descriptorFunction', ...$values); echo call_user_func([$methods, 'instanceValue'], ...$values); echo call_user_func([DescriptorMethods::class, 'staticValue'], ...$values);",
    );
}

/// Traversable unpack remains limited to descriptor invokers until direct-call lowering owns
/// the same runtime iterator walk. Scalar sources remain invalid on the descriptor surface.
#[test]
fn test_descriptor_traversable_spread_does_not_weaken_other_unpack_surfaces() {
    expect_error(
        "<?php class DescriptorValues implements IteratorAggregate { public function getIterator(): Traversable { yield 5; } } function direct($value): int { return $value; } $values = new DescriptorValues(); echo direct(...$values);",
        "Spread operator requires an array",
    );
    expect_error(
        "<?php function descriptorFunction($value): int { return $value; } echo call_user_func('descriptorFunction', ...1);",
        "Spread operator requires an array",
    );
}

/// Verifies that implementing a BUILTIN interface stays valid when the program's `eval()` gives
/// every frame the hidden argument collector.
///
/// A compiler-injected interface signature is synthesized after the `func_args` pass, so it can
/// never carry the collector. Comparing its absence against the implementing method reported a
/// widening the source never wrote, and refused a PHP-valid class.
#[test]
fn test_builtin_interface_implementation_accepts_the_generated_collector() {
    expect_no_error(
        "<?php class Counter implements Iterator { private int $i = 0; public function current(): mixed { return $this->i; } public function key(): mixed { return $this->i; } public function next(): void { $this->i++; } public function rewind(): void { $this->i = 0; } public function valid(): bool { return $this->i < 2; } } eval('return null;'); foreach (new Counter() as $value) { echo $value; }",
    );
}

/// Verifies that overriding a BUILTIN class method stays valid under the same whole-program
/// capture, for the inheritance half of the rule.
#[test]
fn test_builtin_class_override_accepts_the_generated_collector() {
    expect_no_error(
        "<?php class AppDate extends DateTime { public function format(string $format): string { return $format; } } eval('return null;'); echo AppDate::class;",
    );
}

/// Verifies that the origin follows the inherited method, not merely the immediate source parent.
/// `MiddleDate` does not redeclare `format()`, so the contract still originates in the injected
/// `DateTime` schema and has no generated collector to compare with the child method.
#[test]
fn test_transitive_builtin_class_override_accepts_the_generated_collector() {
    expect_no_error(
        "<?php class MiddleDate extends DateTime {} class AppDate extends MiddleDate { public function format(string $format): string { return $format; } } eval('return null;'); echo AppDate::class;",
    );
}

/// Verifies the same per-method origin rule through a source interface that only inherits its
/// method contracts from the compiler-injected `Iterator` interface.
#[test]
fn test_transitive_builtin_interface_contract_accepts_the_generated_collector() {
    expect_no_error(
        "<?php interface AppIterator extends Iterator {} class Counter implements AppIterator { private int $i = 0; public function current(): mixed { return $this->i; } public function key(): mixed { return $this->i; } public function next(): void { $this->i++; } public function rewind(): void { $this->i = 0; } public function valid(): bool { return $this->i < 2; } } eval('return null;'); foreach (new Counter() as $value) { echo $value; }",
    );
}

/// Hidden actual-count storage is a real ABI difference between source declarations. A source
/// variadic that uses `func_num_args()` receives that extra slot, so an override whose parent does
/// not capture the count must remain rejected.
#[test]
fn test_source_variadic_override_rejects_hidden_argc_abi_mismatch() {
    expect_error(
        "<?php class Base { public function countIt(int $first = 0, ...$rest): int { return count($rest); } } class Child extends Base { public function countIt(int $first = 0, ...$rest): int { return func_num_args(); } } echo (new Child())->countIt();",
        "the inherited signature cannot be widened to collect surplus arguments",
    );
}
