//! Purpose:
//! Integration tests for end-to-end codegen coverage of the PHP 8.5 pipe operator (`|>`),
//! covering every RHS form (first-class callable, static method, instance method, closure
//! literal, variable callable, and call returning a callable) plus chaining and precedence.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout.

use crate::support::*;

// Verifies `value |> func(...)` pipes a value through a user function via FCC syntax.
#[test]
fn test_pipe_with_first_class_callable_user_function() {
    let out = compile_and_run(
        r#"<?php
function double(int $n): int { return $n * 2; }
$r = 21 |> double(...);
echo $r;
"#,
    );
    assert_eq!(out, "42");
}

// Verifies `value |> Namespace\func(...)` pipes through a namespaced user function.
#[test]
fn test_pipe_with_namespaced_first_class_callable_user_function() {
    let out = compile_and_run(
        r#"<?php
namespace App;
function double(int $n): int { return $n * 2; }
echo 21 |> double(...);
"#,
    );
    assert_eq!(out, "42");
}

// Verifies `value |> func1(...) |> func2(...)` chains two FCC calls in sequence.
#[test]
fn test_pipe_chained_user_functions() {
    let out = compile_and_run(
        r#"<?php
function double(int $n): int { return $n * 2; }
function increment(int $n): int { return $n + 1; }
$r = 5 |> double(...) |> increment(...);
echo $r;
"#,
    );
    assert_eq!(out, "11");
}

// Verifies `value |> (function($v) { ... })` pipes through an anonymous function literal.
#[test]
fn test_pipe_with_closure_literal() {
    let out = compile_and_run(
        r#"<?php
$r = 3 |> (function($v) { return $v * 4; });
echo $r;
"#,
    );
    assert_eq!(out, "12");
}

// Verifies `value |> (fn($v) => expr)` pipes through a PHP arrow function.
#[test]
fn test_pipe_with_arrow_function() {
    let out = compile_and_run(
        r#"<?php
$r = 7 |> (fn($v) => $v + 100);
echo $r;
"#,
    );
    assert_eq!(out, "107");
}

// Verifies `value |> $cb` pipes through a variable holding a callable.
#[test]
fn test_pipe_with_variable_callable() {
    let out = compile_and_run(
        r#"<?php
function triple(int $n): int { return $n * 3; }
$cb = triple(...);
$r = 4 |> $cb;
echo $r;
"#,
    );
    assert_eq!(out, "12");
}

// Verifies `value |> Class::method(...)` pipes through a static method via FCC syntax.
#[test]
fn test_pipe_with_static_method() {
    let out = compile_and_run(
        r#"<?php
class Calc {
    public static function quad(int $n): int { return $n * 4; }
}
$r = 5 |> Calc::quad(...);
echo $r;
"#,
    );
    assert_eq!(out, "20");
}

// Verifies `value |> $obj->method(...)` pipes through an instance method via FCC syntax.
#[test]
fn test_pipe_with_instance_method() {
    let out = compile_and_run(
        r#"<?php
class Bumper {
    private int $bump;
    public function __construct(int $bump) { $this->bump = $bump; }
    public function apply(int $n): int { return $n + $this->bump; }
}
$b = new Bumper(10);
$r = 7 |> $b->apply(...);
echo $r;
"#,
    );
    assert_eq!(out, "17");
}

// Verifies `5 + 2 |> double(...)` parses as `(5 + 2) |> double(...)` — arithmetic has lower precedence than pipe.
#[test]
fn test_pipe_precedence_with_arithmetic() {
    // 5 + 2 |> double(...) must parse as (5 + 2) |> double(...) = double(7) = 14.
    let out = compile_and_run(
        r#"<?php
function double(int $n): int { return $n * 2; }
echo 5 + 2 |> double(...);
"#,
    );
    assert_eq!(out, "14");
}

// Verifies `"a" . "b" |> wrap(...)` parses as `("a" . "b") |> wrap(...)` — concat has lower precedence than pipe.
#[test]
fn test_pipe_precedence_with_concat() {
    // "a" . "b" |> wrap(...) must parse as ("a" . "b") |> wrap(...).
    let out = compile_and_run(
        r#"<?php
function wrap(string $s): string { return "[" . $s . "]"; }
echo "a" . "b" |> wrap(...);
"#,
    );
    assert_eq!(out, "[ab]");
}

// Verifies `'beep' |> strlen(...) == 4` evaluates as `(strlen('beep') == 4)` — pipe result feeds into comparison.
#[test]
fn test_pipe_precedence_with_comparison() {
    // 'beep' |> strlen(...) == 4 must compute strlen('beep')==4 -> "1".
    let out = compile_and_run(
        r#"<?php
echo (int)('beep' |> strlen(...) == 4);
"#,
    );
    assert_eq!(out, "1");
}

// Verifies `value |> func(...)` uses default parameter values when optional args are omitted.
#[test]
fn test_pipe_with_default_parameters() {
    let out = compile_and_run(
        r#"<?php
function shift(int $n, int $by = 5): int { return $n + $by; }
$r = 10 |> shift(...);
echo $r;
"#,
    );
    assert_eq!(out, "15");
}

// Verifies `value |> strtoupper(...)` pipes through a PHP builtin function via FCC routing.
#[test]
fn test_pipe_with_string_builtin() {
    // strtoupper is a PHP-visible builtin; pipe should route through builtin dispatch.
    let out = compile_and_run(
        r#"<?php
$r = "hello" |> strtoupper(...);
echo $r;
"#,
    );
    assert_eq!(out, "HELLO");
}

// Verifies the LHS `(++$x)` is evaluated once before the RHS call; side effect must happen before pipe argument is read.
#[test]
fn test_pipe_lhs_evaluated_before_rhs_call() {
    // The LHS expression with side effects executes once, before the call.
    let out = compile_and_run(
        r#"<?php
function track(int $n): int { echo "called(" . $n . ")"; return $n; }
$x = 0;
$r = (++$x) |> track(...);
echo "|" . $r;
"#,
    );
    assert_eq!(out, "called(1)|1");
}

// Verifies LHS mutation `$box = $next` is visible to the RHS receiver `$box->read(...)` inside the pipe.
#[test]
fn test_pipe_lhs_mutation_visible_to_rhs_method_receiver() {
    let out = compile_and_run(
        r#"<?php
class Label {
    public function __construct(private string $name) {}
    public function read($ignored): string { return $this->name; }
}
$box = new Label("old");
$next = new Label("new");
echo ($box = $next) |> $box->read(...);
"#,
    );
    assert_eq!(out, "new");
}

// Verifies LHS mutation `$cb = second(...)` is visible to the RHS callable `$cb` inside the pipe.
#[test]
fn test_pipe_lhs_mutation_visible_to_rhs_callable_variable() {
    let out = compile_and_run(
        r#"<?php
function first($value): string { return "first"; }
function second($value): string { return "second"; }
$cb = first(...);
echo ($cb = second(...)) |> $cb;
"#,
    );
    assert_eq!(out, "second");
}

// Verifies `7 |> label(...)` where `label` returns `string` correctly captures the return type in assignment.
#[test]
fn test_pipe_result_string_assignment_uses_callable_return_type() {
    let out = compile_and_run(
        r#"<?php
function label(int $n): string { return "v" . $n; }
$result = 7 |> label(...);
echo $result;
"#,
    );
    assert_eq!(out, "v7");
}

// Verifies `(3 |> double(...)) + 4` treats pipe as a grouped sub-expression in arithmetic context.
#[test]
fn test_pipe_in_arithmetic_context() {
    let out = compile_and_run(
        r#"<?php
function double(int $n): int { return $n * 2; }
echo (3 |> double(...)) + 4;
"#,
    );
    assert_eq!(out, "10");
}

// Compiles a PHP source string to assembly in an isolated temp directory.
//
// Returns `(user_asm, libs, dir)` where `user_asm` is the compiled user code assembly,
// `libs` are required runtime libraries, and `dir` is the temp directory to clean up.
fn compile_pipe_fixture(source: &str, label: &str) -> (String, Vec<String>, std::path::PathBuf) {
    let dir = make_cli_test_dir(label);
    let (user_asm, _runtime_asm, libs) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    (user_asm, libs, dir)
}

// Verifies FCC variable short-circuit emits `bl _fn_triple` directly and stubs the wrapper with `uninvoked FCC wrapper`.
// Regression guard for the FCC variable short-circuit optimisation and dead-wrapper elimination.
#[test]
fn test_pipe_with_fcc_variable_function_target_emits_direct_call_and_stubs_wrapper() {
    // Asm-level guard for the FCC variable short-circuit AND the dead-wrapper
    // optimisation. When `$cb = triple(...)` and the pipe target is `$cb`, the
    // call must reach `_fn_triple` directly (`bl _fn_triple`) instead of
    // routing through the closure wrapper. Additionally, because the FCC
    // value never escapes the short-circuit, the wrapper body itself is
    // replaced by a tiny `ret` stub — both signals must be present, otherwise
    // either the call-site optimisation or the dead-wrapper elimination has
    // regressed.
    let dir = make_cli_test_dir("elephc_pipe_fcc_short_circuit_direct_call");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function triple(int $n): int { return $n * 3; }
$cb = triple(...);
echo 14 |> $cb;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        user_asm.contains("bl _fn_triple\n"),
        "expected `bl _fn_triple` at the short-circuit call site; the FCC variable short-circuit may have regressed:\n{}",
        user_asm
    );
    assert!(
        user_asm.contains("uninvoked FCC wrapper"),
        "expected the FCC wrapper to be stubbed when the value never escapes; the dead-wrapper optimisation may have regressed:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "42");

    let _ = fs::remove_dir_all(&dir);
}

// Verifies `Class::method(...)` FCC short-circuit emits `bl _static_<Class>_<method>` directly and stubs the wrapper.
// Regression guard for the named static method short-circuit and dead-wrapper elimination.
#[test]
fn test_pipe_with_fcc_variable_static_method_named_target_emits_direct_call_and_stubs_wrapper() {
    // `Class::method(...)` short-circuit must lower to a direct `bl
    // _static_<Class>_<method>` and stub the deferred wrapper.
    let (user_asm, libs, dir) = compile_pipe_fixture(
        r#"<?php
class Calc {
    public static function quad(int $n): int { return $n * 4; }
}
$cb = Calc::quad(...);
echo 5 |> $cb;
"#,
        "elephc_pipe_static_named_direct_call",
    );

    assert!(
        user_asm.contains("bl _static_Calc_quad\n"),
        "expected `bl _static_Calc_quad` at the short-circuit call site:\n{}",
        user_asm
    );
    assert!(
        user_asm.contains("uninvoked FCC wrapper"),
        "expected the FCC wrapper to be stubbed:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &libs,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "20");
    let _ = fs::remove_dir_all(&dir);
}

// Verifies `self::method(...)` resolves to a Named receiver and emits `bl _static_<Class>_<method>` directly;
// confirms the wrapper is stubbed for self:: static targets. Regression guard.
#[test]
fn test_pipe_with_fcc_variable_self_static_method_resolves_and_stubs_wrapper() {
    // `self::method(...)` is resolved to a Named receiver at storage time and
    // the call lowers identically to the Named case (direct `bl _static_<Class>_<method>`).
    let (user_asm, libs, dir) = compile_pipe_fixture(
        r#"<?php
class Marker {
    public static function tag(string $s): string { return "[" . $s . "]"; }
    public static function wrap(string $s): string {
        $cb = self::tag(...);
        return $s |> $cb;
    }
}
echo Marker::wrap("ok");
"#,
        "elephc_pipe_self_static_direct_call",
    );

    assert!(
        user_asm.contains("bl _static_Marker_tag\n"),
        "expected `bl _static_Marker_tag` at the short-circuit call site:\n{}",
        user_asm
    );
    assert!(
        user_asm.contains("uninvoked FCC wrapper"),
        "expected the FCC wrapper to be stubbed for self:: target:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &libs,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "[ok]");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies instance method FCC variables use descriptor invokers so captured
/// receiver environments are read from descriptor storage rather than source locals.
#[test]
fn test_pipe_with_fcc_variable_method_target_uses_descriptor_invoker() {
    let (user_asm, libs, dir) = compile_pipe_fixture(
        r#"<?php
class Bumper {
    public function __construct(private int $bump) {}
    public function apply(int $n): int { return $n + $this->bump; }
}
$b = new Bumper(10);
$cb = $b->apply(...);
echo 7 |> $cb;
"#,
        "elephc_pipe_method_descriptor_invoker",
    );

    assert!(
        user_asm.contains("call descriptor variable $cb()")
            && user_asm.contains("callable_invoker"),
        "expected instance method FCC variables to route through descriptor invokers:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &libs,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "17");
    let _ = fs::remove_dir_all(&dir);
}

// Verifies chained FCC variables `5 |> $a |> $b` stub both wrappers and emit direct calls to each stage.
// Regression guard for the optimisation composing across chained pipe stages.
#[test]
fn test_pipe_chained_fcc_variables_stub_every_wrapper() {
    // Each FCC variable in the chain has its own wrapper. With short-circuit
    // firing for both, both wrappers must be stubbed — verifying the
    // optimisation composes across chained pipe stages.
    let (user_asm, libs, dir) = compile_pipe_fixture(
        r#"<?php
function double(int $n): int { return $n * 2; }
function increment(int $n): int { return $n + 1; }
$a = double(...);
$b = increment(...);
echo 5 |> $a |> $b;
"#,
        "elephc_pipe_chained_stubs",
    );

    let stubs = user_asm.matches("uninvoked FCC wrapper").count();
    assert!(
        stubs >= 2,
        "expected at least 2 stubbed FCC wrappers (one per FCC variable), got {}:\n{}",
        stubs,
        user_asm
    );
    assert!(
        user_asm.contains("bl _fn_double\n") && user_asm.contains("bl _fn_increment\n"),
        "expected direct calls to both pipe stages:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &libs,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "11");
    let _ = fs::remove_dir_all(&dir);
}
