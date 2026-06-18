//! Purpose:
//! Interpreter tests for PHP language-construct builtins that are also visible
//! through eval's builtin registry.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - `exit` and `die` are probed for visibility only because executing them
//!   terminates the current process.

use super::super::*;
use super::support::*;

/// Verifies eval language-construct builtins are visible and direct semantics still work.
#[test]
fn execute_program_language_construct_builtins_are_visible() {
    let program = parse_fragment(
        br#"$x = 1;
$y = 2;
unset($x);
unset($y, $missing);
echo isset($x) ? "bad" : "unset"; echo ":";
echo isset($y) ? "bad" : "multi"; echo ":";
echo empty($missing) ? "empty" : "bad"; echo ":";
echo empty(0) ? "zero" : "bad"; echo ":";
echo function_exists("isset") ? "I" : "i";
echo function_exists("empty") ? "E" : "e";
echo function_exists("unset") ? "U" : "u";
echo function_exists("exit") ? "X" : "x";
echo function_exists("die") ? "D" : "d";
echo is_callable("isset") ? "i" : "!";
echo is_callable("empty") ? "e" : "!";
echo is_callable("unset") ? "u" : "!";
echo is_callable("exit") ? "x" : "!";
return is_callable("die");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "unset:multi:empty:zero:IEUXDieux");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies callable dispatch for safe language-construct builtins uses materialized values.
#[test]
fn execute_program_language_construct_builtins_dispatch_as_callables() {
    let program = parse_fragment(
        br#"echo call_user_func("isset", 1, null) ? "bad" : "null"; echo ":";
echo call_user_func("isset", 1, "x") ? "isset" : "bad"; echo ":";
echo call_user_func("empty", 0) ? "empty" : "bad"; echo ":";
echo call_user_func_array("empty", ["value" => "x"]) ? "bad" : "filled"; echo ":";
$result = call_user_func("unset", 1);
echo is_null($result) ? "unset-null" : "bad"; echo ":";
echo call_user_func_array("isset", ["var" => 1, "vars" => "x"]) ? "named" : "bad";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "null:isset:empty:filled:unset-null:named");
    assert_eq!(values.get(result), FakeValue::Null);
}

/// Verifies `isset()` without operands is rejected like the main compiler.
#[test]
fn execute_program_isset_without_arguments_fails() {
    let program = parse_fragment(br#"isset();"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values).expect_err("isset arity fails");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
