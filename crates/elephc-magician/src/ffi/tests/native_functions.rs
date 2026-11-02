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

/// Function attribute records retain named arguments, source order and repeated entries.
#[test]
fn register_native_function_attributes_preserves_metadata() {
    let mut ctx = ElephcEvalContext::new();
    let name = b"Native\\DeprecatedFn";
    assert_eq!(unsafe { __elephc_eval_register_native_function(
        &mut ctx, name.as_ptr(), name.len() as u64,
        1usize as *mut c_void, Some(fake_native_invoker), 0,
    ) }, 1);
    let args = vec![EvalAttributeArg::Named {
        name: "since".into(), value: Box::new(EvalAttributeArg::String("8.5".into())),
    }];
    let record = native_member_attribute_record(4, "\\NATIVE\\DeprecatedFn", "Deprecated", Some(&args));
    for _ in 0..2 {
        assert_eq!(unsafe { __elephc_eval_register_native_member_attribute(
            &mut ctx, record.as_ptr(), record.len() as u64,
        ) }, 1);
    }
    let function = ctx.native_function("native\\deprecatedfn").unwrap();
    assert_eq!(function.attributes().len(), 2);
    assert_eq!(function.attributes()[0].name(), "Deprecated");
    assert_eq!(function.attributes()[0].args(), Some(args.as_slice()));
    let missing = native_member_attribute_record(4, "missing", "Deprecated", Some(&[]));
    assert_eq!(unsafe { __elephc_eval_register_native_member_attribute(
        &mut ctx, missing.as_ptr(), missing.len() as u64,
    ) }, 0);
}

/// The shape ABI distinguishes compiler-internal declarations from user and legacy callbacks.
#[test]
fn register_native_function_shape_preserves_declaration_origin() {
    let mut ctx = ElephcEvalContext::new();
    let name = b"opcache_reset";
    let registered = unsafe {
        __elephc_eval_register_native_function(
            &mut ctx, name.as_ptr(), name.len() as u64,
            1usize as *mut c_void, Some(fake_native_invoker), 0,
        )
    };
    assert_eq!(registered, 1);
    assert!(!ctx.native_function("opcache_reset").unwrap().is_internal());
    for (flags, expected) in [
        (crate::context::NATIVE_SHAPE_FLAG_INTERNAL, true),
        (0, false),
        (1 << 20, false),
    ] {
        let registered = unsafe {
            __elephc_eval_register_native_function_shape(
                &mut ctx, name.as_ptr(), name.len() as u64, 0, 0, flags,
            )
        };
        assert_eq!(registered, 1);
        assert_eq!(ctx.native_function("opcache_reset").unwrap().is_internal(), expected);
    }
}

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

/// Verifies the by-reference-return registration marks only the named native function, and
/// fails closed for a function that was never registered.
#[test]
fn register_native_function_returns_reference_marks_the_declaration() {
    let mut ctx = ElephcEvalContext::new();
    let descriptor = 1usize as *mut c_void;
    for name in [b"REF_PROBE".as_slice(), b"value_probe".as_slice()] {
        let registered = unsafe {
            __elephc_eval_register_native_function(
                &mut ctx,
                name.as_ptr(),
                name.len() as u64,
                descriptor,
                Some(fake_native_invoker),
                0,
            )
        };
        assert_eq!(registered, 1);
    }
    let marked = unsafe {
        __elephc_eval_register_native_function_returns_reference(
            &mut ctx,
            b"REF_PROBE".as_ptr(),
            9,
        )
    };
    let missing = unsafe {
        __elephc_eval_register_native_function_returns_reference(
            &mut ctx,
            b"missing_probe".as_ptr(),
            13,
        )
    };

    assert_eq!(marked, 1);
    assert_eq!(missing, 0);
    assert!(ctx.native_function("ref_probe").expect("registered").returns_reference());
    assert!(!ctx.native_function("value_probe").expect("registered").returns_reference());
}
