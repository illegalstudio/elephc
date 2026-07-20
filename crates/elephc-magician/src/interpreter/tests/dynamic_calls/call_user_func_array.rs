//! Purpose:
//! Interpreter tests for `call_user_func_array` dispatch, named argument
//! normalization, temporary cleanup, and duplicate declarations.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Eval, builtin, and registered native function targets share this surface.

use super::super::super::*;
use super::super::support::*;

/// Verifies `call_user_func_array` inside eval can dispatch an eval-declared function.
#[test]
fn execute_program_call_user_func_array_dispatches_declared_function() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return $x + $y; }
return call_user_func_array("dyn", [4, 5]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(9));
}
/// Verifies `call_user_func_array` string keys bind eval-declared parameters by name.
#[test]
fn execute_program_call_user_func_array_binds_declared_named_args() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return ($x * 10) + $y; }
return call_user_func_array("dyn", ["y" => 2, "x" => 1]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(12));
}
/// Verifies context-level `call_user_func_array` dispatch binds eval-declared named args.
#[test]
fn execute_context_function_call_array_binds_declared_named_args() {
    let program = parse_fragment(br#"function dyn($x, $y) { return ($x * 10) + $y; }"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let _ = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");
    let arg_array = values.assoc_new(2).expect("allocate argument array");
    let key_y = values.string("y").expect("allocate y key");
    let value_y = values.int(2).expect("allocate y value");
    let _ = values
        .array_set(arg_array, key_y, value_y)
        .expect("store y argument");
    let key_x = values.string("x").expect("allocate x key");
    let value_x = values.int(1).expect("allocate x value");
    let _ = values
        .array_set(arg_array, key_x, value_x)
        .expect("store x argument");

    let result = execute_context_function_call_array(&mut context, "dyn", arg_array, &mut values)
        .expect("execute context function call array");

    assert_eq!(values.get(result), FakeValue::Int(12));
}
/// Verifies `call_user_func_array` rejects positional values after named keys.
#[test]
fn execute_program_call_user_func_array_rejects_positional_after_named_arg() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return $x + $y; }
return call_user_func_array("dyn", ["y" => 2, 1]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values);

    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
}
/// Verifies `call_user_func_array` inside eval can dispatch a supported builtin.
#[test]
fn execute_program_call_user_func_array_dispatches_builtin() {
    let program = parse_fragment(br#"return call_user_func_array("strlen", ["abcd"]);"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(4));
}
/// Verifies `call_user_func_array` releases literal callback and argument-array temporaries.
#[test]
fn execute_program_call_user_func_array_releases_literal_callback_and_arg_array() {
    let program = parse_fragment(br#"return call_user_func_array("strlen", ["abcd"]);"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(4));
    assert!(
        values
            .releases
            .iter()
            .any(|release| values.get(*release) == FakeValue::String("strlen".to_string())),
        "literal callback string should be released after dispatch"
    );
    assert!(
        values
            .releases
            .iter()
            .any(|release| matches!(values.get(*release), FakeValue::Array(_))),
        "literal call argument array should be released after dispatch"
    );
}
/// Verifies `call_user_func_array` releases literal temporaries after a fatal dispatch.
#[test]
fn execute_program_call_user_func_array_releases_literal_temporaries_after_fatal() {
    let program =
        parse_fragment(br#"return call_user_func_array("strlen", ["unknown" => "abcd"]);"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values);

    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
    assert!(
        values
            .releases
            .iter()
            .any(|release| values.get(*release) == FakeValue::String("strlen".to_string())),
        "literal callback string should be released after fatal dispatch"
    );
    assert!(
        values
            .releases
            .iter()
            .any(|release| matches!(values.get(*release), FakeValue::Assoc(_))),
        "literal call argument hash should be released after fatal dispatch"
    );
}
/// Verifies `call_user_func_array` releases literal callback when arg-array evaluation fails.
#[test]
fn execute_program_call_user_func_array_releases_literal_callback_after_arg_array_eval_fatal() {
    let program = parse_fragment(br#"return call_user_func_array("strlen", MISSING_ARG_ARRAY);"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values);

    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
    assert!(
        values
            .releases
            .iter()
            .any(|release| values.get(*release) == FakeValue::String("strlen".to_string())),
        "literal callback string should be released when argument-array evaluation fails"
    );
}
/// Verifies `call_user_func_array` inside eval can dispatch a registered native function.
#[test]
fn execute_program_call_user_func_array_dispatches_registered_native_function() {
    let program = parse_fragment(br#"return call_user_func_array("native_answer", [4, 5]);"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let native = NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}
/// Verifies `call_user_func_array` named keys can bind registered native parameters.
#[test]
fn execute_program_call_user_func_array_binds_registered_native_named_args() {
    let program = parse_fragment(
        br#"return call_user_func_array("native_answer", ["right" => 2, "left" => 1]);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let mut native =
        NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
    assert!(native.set_param_name(0, "left"));
    assert!(native.set_param_name(1, "right"));
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}
/// Verifies duplicate eval-declared function names fail in a shared context.
#[test]
fn execute_program_rejects_duplicate_declared_function() {
    let define = parse_fragment(br#"function dyn() { return 1; }"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program_with_context(&mut context, &define, &mut scope, &mut values)
        .expect("execute first declaration");
    let err = execute_program_with_context(&mut context, &define, &mut scope, &mut values)
        .expect_err("duplicate function declaration should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
