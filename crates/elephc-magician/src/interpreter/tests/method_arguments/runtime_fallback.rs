//! Purpose:
//! Interpreter tests for runtime/AOT method argument fallback, named binding,
//! by-reference validation, coercion writeback, and fatal paths.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Instance and static runtime hooks preserve writeback before errors.

use super::super::super::*;
use super::super::support::*;

/// Verifies runtime/AOT method fallback binds registered native method named arguments.
#[test]
fn execute_program_binds_registered_runtime_method_named_args() {
    let program = parse_fragment(
        br#"$box = new KnownClass(10);
return $box->add2_x(right: 2, left: 3);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(2);
    assert!(signature.set_param_name(0, "left"));
    assert!(signature.set_param_name(1, "right"));
    assert!(context.define_native_method_signature("KnownClass", "add2_x", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("registered runtime method named args should bind");

    assert_eq!(values.get(result), FakeValue::Int(15));
}

/// Verifies runtime/AOT method fallback honors registered by-reference parameter metadata.
#[test]
fn execute_program_rejects_runtime_method_by_ref_temporary_arg() {
    let program = parse_fragment(
        br#"$box = new KnownClass(10);
return $box->add2_x(1, 2);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(2);
    assert!(signature.set_param_name(0, "left"));
    assert!(signature.set_param_name(1, "right"));
    assert!(signature.set_param_by_ref(0, true));
    assert!(context.define_native_method_signature("KnownClass", "add2_x", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect_err("literal cannot satisfy a runtime by-reference method parameter");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies runtime/AOT method fallback writes coerced by-reference args back.
#[test]
fn execute_program_writes_back_runtime_method_by_ref_type_coercion() {
    let program = parse_fragment(
        br#"$box = new KnownClass(10);
$value = "3";
echo $box->add2_x($value, 2);
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
        .expect("registered runtime method by-ref coercion should bind");

    assert_eq!(values.output, "15");
    assert_eq!(values.get(result), FakeValue::Int(3));
}

/// Verifies AOT instance method by-reference writeback still runs when the method fatals.
#[test]
fn execute_program_writes_back_runtime_method_by_ref_before_fatal() {
    let program = parse_fragment(
        br#"$box = new KnownClass(10);
$value = "3";
$box->add2_x($value);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(1);
    assert!(signature.set_param_name(0, "left"));
    assert!(signature.set_param_type(
        0,
        EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false)
    ));
    assert!(signature.set_param_by_ref(0, true));
    assert!(context.define_native_method_signature("KnownClass", "add2_x", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect_err("runtime method should fail after argument binding");
    let value = scope
        .entry("value")
        .expect("caller variable should remain visible")
        .cell();

    assert_eq!(err, EvalStatus::UnsupportedConstruct);
    assert_eq!(values.get(value), FakeValue::Int(3));
}

/// Verifies AOT static method by-reference writeback still runs when the method fatals.
#[test]
fn execute_program_writes_back_runtime_static_method_by_ref_before_fatal() {
    let program = parse_fragment(
        br#"$value = "3";
KnownClass::sum($value);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(1);
    assert!(signature.set_param_name(0, "left"));
    assert!(signature.set_param_type(
        0,
        EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false)
    ));
    assert!(signature.set_param_by_ref(0, true));
    assert!(context.define_native_static_method_signature("KnownClass", "sum", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect_err("runtime static method should fail after argument binding");
    let value = scope
        .entry("value")
        .expect("caller variable should remain visible")
        .cell();

    assert_eq!(err, EvalStatus::UnsupportedConstruct);
    assert_eq!(values.get(value), FakeValue::Int(3));
}

/// Verifies runtime/AOT method fallback rejects named arguments without metadata.
#[test]
fn execute_program_rejects_unregistered_named_args_for_runtime_method_fallback() {
    let program =
        parse_fragment(br#"return $this->answer(value: 1);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let object = values.alloc(FakeValue::Object(Vec::new()));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("unregistered runtime method fallback named args should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
