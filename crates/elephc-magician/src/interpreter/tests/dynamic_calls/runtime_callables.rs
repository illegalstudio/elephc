//! Purpose:
//! Interpreter tests for runtime AOT static/method callable hooks, named/default
//! arguments, by-reference constraints, and writeback.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Runtime bridge failures and coercion writeback are asserted separately.

use super::super::super::*;
use super::super::support::*;

/// Verifies static calls fall back to runtime AOT hooks when no eval class matches.
#[test]
fn execute_program_static_call_dispatches_runtime_method_hook() {
    let program = parse_fragment(
        br#"echo KnownClass::join("A", "B"); echo ":";
$cb = ["KnownClass", "join"];
echo call_user_func($cb, "C", "D"); echo ":";
$named = "KnownClass::join";
echo $named("E", "F"); echo ":";
return call_user_func_array(["KnownClass", "sum"], [2, 5]);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut join_signature = NativeCallableSignature::new(2);
    assert!(join_signature.set_param_name(0, "left"));
    assert!(join_signature.set_param_name(1, "right"));
    assert!(context.define_native_static_method_signature(
        "KnownClass",
        "join",
        join_signature
    ));
    let mut sum_signature = NativeCallableSignature::new(2);
    assert!(sum_signature.set_param_name(0, "left"));
    assert!(sum_signature.set_param_name(1, "right"));
    assert!(context.define_native_static_method_signature(
        "KnownClass",
        "sum",
        sum_signature
    ));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.output, "AB:CD:EF:");
    assert_eq!(values.get(result), FakeValue::Int(7));
}

/// Verifies runtime AOT static method fallback binds registered named arguments.
#[test]
fn execute_program_static_runtime_method_hook_binds_named_args() {
    let program = parse_fragment(
        br#"return call_user_func_array(["KnownClass", "join"], ["right" => "B", "left" => "A"]);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(2);
    assert!(signature.set_param_name(0, "left"));
    assert!(signature.set_param_name(1, "right"));
    assert!(context.define_native_static_method_signature("KnownClass", "join", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("registered named AOT call should bind");

    assert_eq!(values.get(result), FakeValue::String("AB".to_string()));
}

/// Verifies runtime AOT static method fallback fills registered scalar defaults.
#[test]
fn execute_program_static_runtime_method_hook_binds_default_args() {
    let program = parse_fragment(
        br#"echo KnownClass::join("A"); echo ":";
return call_user_func_array(["KnownClass", "join"], ["left" => "C"]);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(2);
    assert!(signature.set_param_name(0, "left"));
    assert!(signature.set_param_name(1, "right"));
    assert!(signature.set_param_default(1, NativeCallableDefault::String("B".to_string())));
    assert!(context.define_native_static_method_signature("KnownClass", "join", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("registered AOT defaults should bind");

    assert_eq!(values.output, "AB:");
    assert_eq!(values.get(result), FakeValue::String("CB".to_string()));
}

/// Verifies runtime AOT static method fallback honors by-reference parameter metadata.
#[test]
fn execute_program_static_runtime_method_hook_rejects_by_ref_temporary_arg() {
    let program = parse_fragment(br#"return KnownClass::sum(1, 2);"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(2);
    assert!(signature.set_param_name(0, "left"));
    assert!(signature.set_param_name(1, "right"));
    assert!(signature.set_param_by_ref(0, true));
    assert!(context.define_native_static_method_signature("KnownClass", "sum", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect_err("literal cannot satisfy a static by-reference method parameter");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies callable-array AOT method dispatch preserves by-reference writeback.
#[test]
fn execute_program_callable_array_runtime_method_writes_back_by_ref_type_coercion() {
    let program = parse_fragment(
        br#"$box = new KnownClass(10);
$cb = [$box, "add2_x"];
$value = "3";
echo $cb($value, 2);
return $value;"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(2);
    assert!(signature.set_param_name(0, "left"));
    assert!(signature.set_param_name(1, "right"));
    assert!(signature.set_param_type(
        0,
        EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false)
    ));
    assert!(signature.set_param_by_ref(0, true));
    assert!(context.define_native_method_signature("KnownClass", "add2_x", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("callable-array runtime method should preserve by-ref target");

    assert_eq!(values.output, "15");
    assert_eq!(values.get(result), FakeValue::Int(3));
}

/// Verifies `call_user_func_array()` preserves by-reference array elements for AOT methods.
#[test]
fn execute_program_call_user_func_array_runtime_method_writes_back_by_ref_type_coercion() {
    let program = parse_fragment(
        br#"$box = new KnownClass(10);
$cb = [$box, "add2_x"];
$value = "3";
echo call_user_func_array($cb, [&$value, 2]);
echo ":";
return $value;"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(2);
    assert!(signature.set_param_name(0, "left"));
    assert!(signature.set_param_name(1, "right"));
    assert!(signature.set_param_type(
        0,
        EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false)
    ));
    assert!(signature.set_param_by_ref(0, true));
    assert!(context.define_native_method_signature("KnownClass", "add2_x", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("call_user_func_array should preserve by-ref array element target");

    assert_eq!(values.output, "15:");
    assert_eq!(values.get(result), FakeValue::Int(3));
}

/// Verifies string and first-class AOT static callables preserve by-reference writeback.
#[test]
fn execute_program_static_runtime_callables_write_back_by_ref_type_coercion() {
    let program = parse_fragment(
        br#"$string = "KnownClass::sum";
$left = "3";
echo $string($left, 2); echo ":";
echo $left + 1; echo ":";
$first = KnownClass::sum(...);
$next = "4";
echo $first($next, 5); echo ":";
return $next;"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(2);
    assert!(signature.set_param_name(0, "left"));
    assert!(signature.set_param_name(1, "right"));
    assert!(signature.set_param_type(
        0,
        EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false)
    ));
    assert!(signature.set_param_by_ref(0, true));
    assert!(context.define_native_static_method_signature("KnownClass", "sum", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("runtime static callables should preserve by-ref target");

    assert_eq!(values.output, "5:4:9:");
    assert_eq!(values.get(result), FakeValue::Int(4));
}

/// Verifies runtime AOT static method fallback rejects named arguments without metadata.
#[test]
fn execute_program_static_runtime_method_hook_rejects_unregistered_named_args() {
    let program = parse_fragment(
        br#"return call_user_func_array(["KnownClass", "join"], ["right" => "B", "left" => "A"]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let error =
        execute_program(&program, &mut scope, &mut values).expect_err("named AOT call should fail");

    assert_eq!(error, EvalStatus::UncaughtThrowable);
}
