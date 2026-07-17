//! Purpose:
//! Interpreter tests for eval closure literals, captures, and callable dispatch.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases exercise eval-only closure execution against fake runtime cells.
//! - Closure values are PHP-visible `Closure` objects with eval-retained bodies.

use super::super::*;
use super::support::*;

/// Verifies eval closure literals dispatch through direct variable calls and call_user_func_array.
#[test]
fn execute_program_dispatches_eval_closure_literal() {
    let program = parse_fragment(
        br#"$fn = function($left, $right = 2) { return $left + $right; };
echo $fn(3); echo ":";
echo call_user_func_array($fn, ["right" => 6, "left" => 5]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "5:11");
}

/// Verifies by-value eval closure captures snapshot the defining value for each invocation.
#[test]
fn execute_program_closure_by_value_capture_uses_snapshot() {
    let program = parse_fragment(
        br#"$x = 1;
$fn = function() use ($x) { $x += 1; return $x; };
$x = 5;
echo $fn(); echo ":";
echo $fn(); echo ":";
echo $x;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");

    assert_eq!(values.output, "2:2:5");
    assert_eq!(values.get(x), FakeValue::Int(5));
}

/// Verifies by-reference eval closure captures write back before a failing body escapes.
#[test]
fn execute_program_closure_by_ref_capture_writes_back_before_fatal() {
    let program = parse_fragment(
        br#"$x = 1;
$fn = function() use (&$x) { $x = 9; missing_eval_closure_function(); };
$fn();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values).expect_err("closure should fail");
    let x = scope.visible_cell("x").expect("scope should contain x");

    assert_eq!(err, EvalStatus::UnsupportedConstruct);
    assert_eq!(values.get(x), FakeValue::Int(9));
}

/// Verifies eval closure by-reference parameters mutate the caller variable.
#[test]
fn execute_program_closure_by_ref_parameter_writes_back() {
    let program = parse_fragment(
        br#"$fn = function(&$value) { $value += 2; };
$value = 3;
$fn($value);
return $value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(5));
}

/// Verifies eval closure values are callable but do not leak into function_exists.
#[test]
fn execute_program_closure_is_callable_without_function_exists_leak() {
    let program = parse_fragment(
        br#"$fn = function() { return "ok"; };
echo is_callable($fn) ? "C" : "c";
echo call_user_func($fn);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");
    let fn_cell = scope.visible_cell("fn").expect("scope should contain fn");
    let FakeValue::Object(_) = values.get(fn_cell) else {
        panic!("closure representation should be a PHP object");
    };
    let identity = values
        .object_identity(fn_cell)
        .expect("closure object should have identity");
    let name = context
        .closure_object_name(identity)
        .expect("closure object should map to callable name");

    assert_eq!(values.output, "Cok");
    assert!(context.has_closure(name));
    assert!(!context.has_function(name));
}
