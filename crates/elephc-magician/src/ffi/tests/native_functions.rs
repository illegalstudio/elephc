//! Purpose:
//! Tests native function signature, parameter, type, and default registration
//! through the eval C ABI.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Registration metadata is read back from the same eval context.

use super::*;

/// Verifies native AOT registration records function parameter metadata and defaults.
#[test]
fn register_native_function_reports_function_exists() {
    let mut ctx = ElephcEvalContext::new();
    let name = b"NATIVE_PROBE";
    let param = b"value";
    let variadic = b"items";
    let param_type = b"?string";
    let return_type = b"int";
    let descriptor = 1usize as *mut c_void;

    let registered = unsafe {
        __elephc_eval_register_native_function(
            &mut ctx,
            name.as_ptr(),
            name.len() as u64,
            descriptor,
            Some(fake_native_invoker),
            2,
        )
    };
    let param_registered = unsafe {
        __elephc_eval_register_native_function_param(
            &mut ctx,
            name.as_ptr(),
            name.len() as u64,
            0,
            param.as_ptr(),
            param.len() as u64,
        )
    };
    let variadic_param_registered = unsafe {
        __elephc_eval_register_native_function_param(
            &mut ctx,
            name.as_ptr(),
            name.len() as u64,
            1,
            variadic.as_ptr(),
            variadic.len() as u64,
        )
    };
    let bridge_support_registered = unsafe {
        __elephc_eval_register_native_function_bridge_support(
            &mut ctx,
            name.as_ptr(),
            name.len() as u64,
            0,
        )
    };
    let param_flags_registered = unsafe {
        __elephc_eval_register_native_function_param_flags(
            &mut ctx,
            name.as_ptr(),
            name.len() as u64,
            0,
            1,
            0,
        )
    };
    let variadic_flags_registered = unsafe {
        __elephc_eval_register_native_function_param_flags(
            &mut ctx,
            name.as_ptr(),
            name.len() as u64,
            1,
            0,
            1,
        )
    };
    let param_type_registered = unsafe {
        __elephc_eval_register_native_function_param_type(
            &mut ctx,
            name.as_ptr(),
            name.len() as u64,
            0,
            param_type.as_ptr(),
            param_type.len() as u64,
        )
    };
    let return_type_registered = unsafe {
        __elephc_eval_register_native_function_return_type(
            &mut ctx,
            name.as_ptr(),
            name.len() as u64,
            return_type.as_ptr(),
            return_type.len() as u64,
        )
    };
    let param_default_registered = unsafe {
        __elephc_eval_register_native_function_param_default_scalar(
            &mut ctx,
            name.as_ptr(),
            name.len() as u64,
            0,
            TEST_NATIVE_DEFAULT_INT,
            42,
        )
    };
    let exists = unsafe { __elephc_eval_function_exists(&ctx, b"native_probe".as_ptr(), 12) };

    assert_eq!(registered, 1);
    let native = ctx
        .native_function("native_probe")
        .expect("native function should be registered");

    assert_eq!(param_registered, 1);
    assert_eq!(variadic_param_registered, 1);
    assert_eq!(bridge_support_registered, 1);
    assert_eq!(param_flags_registered, 1);
    assert_eq!(variadic_flags_registered, 1);
    assert_eq!(param_type_registered, 1);
    assert_eq!(return_type_registered, 1);
    assert_eq!(param_default_registered, 1);
    assert_eq!(exists, 1);
    assert_eq!(
        native.param_names(),
        &["value".to_string(), "items".to_string()]
    );
    let native_type = native.param_type(0).expect("native parameter type");
    assert!(native_type.allows_null());
    assert_eq!(native_type.variants(), &[EvalParameterTypeVariant::String]);
    assert!(native.param_by_ref(0));
    assert!(!native.param_variadic(0));
    assert!(native.param_variadic(1));
    assert!(!native.bridge_supported());
    assert_eq!(native.required_param_count(), 0);
    assert_eq!(
        native.return_type().expect("native return type").variants(),
        &[EvalParameterTypeVariant::Int]
    );
    assert_eq!(native.param_default(0), Some(&NativeCallableDefault::Int(42)));
}
