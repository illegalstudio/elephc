//! Purpose:
//! Tests native instance, static, constructor, interface, and abstract member
//! signature registration through the eval C ABI.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Parameters, flags, types, defaults, and bridge support are checked together.

use super::*;

/// Verifies native AOT method registration records instance/static/constructor parameters.
#[test]
fn register_native_methods_record_signature_metadata() {
    let mut ctx = ElephcEvalContext::new();
    let method = b"KnownClass::join";
    let static_method = b"KnownClass::sum";
    let class = b"KnownClass";
    let left = b"left";
    let right = b"right";
    let value = b"value";
    let method_type = b"int|string|null";
    let static_type = b"?string";
    let constructor_type = b"KnownDep";
    let return_type = b"bool";

    let method_registered = unsafe {
        __elephc_eval_register_native_method(&mut ctx, method.as_ptr(), method.len() as u64, 2)
    };
    let method_param_registered = unsafe {
        __elephc_eval_register_native_method_param(
            &mut ctx,
            method.as_ptr(),
            method.len() as u64,
            1,
            right.as_ptr(),
            right.len() as u64,
        )
    };
    let method_param_type_registered = unsafe {
        __elephc_eval_register_native_method_param_type(
            &mut ctx,
            method.as_ptr(),
            method.len() as u64,
            0,
            method_type.as_ptr(),
            method_type.len() as u64,
        )
    };
    let method_param_flags_registered = unsafe {
        __elephc_eval_register_native_method_param_flags(
            &mut ctx,
            method.as_ptr(),
            method.len() as u64,
            1,
            1,
            0,
        )
    };
    let static_registered = unsafe {
        __elephc_eval_register_native_static_method(
            &mut ctx,
            static_method.as_ptr(),
            static_method.len() as u64,
            2,
        )
    };
    let static_param_registered = unsafe {
        __elephc_eval_register_native_static_method_param(
            &mut ctx,
            static_method.as_ptr(),
            static_method.len() as u64,
            0,
            left.as_ptr(),
            left.len() as u64,
        )
    };
    let static_param_type_registered = unsafe {
        __elephc_eval_register_native_static_method_param_type(
            &mut ctx,
            static_method.as_ptr(),
            static_method.len() as u64,
            0,
            static_type.as_ptr(),
            static_type.len() as u64,
        )
    };
    let static_param_flags_registered = unsafe {
        __elephc_eval_register_native_static_method_param_flags(
            &mut ctx,
            static_method.as_ptr(),
            static_method.len() as u64,
            1,
            0,
            1,
        )
    };
    let static_return_type_registered = unsafe {
        __elephc_eval_register_native_static_method_return_type(
            &mut ctx,
            static_method.as_ptr(),
            static_method.len() as u64,
            return_type.as_ptr(),
            return_type.len() as u64,
        )
    };
    let constructor_registered = unsafe {
        __elephc_eval_register_native_constructor(&mut ctx, class.as_ptr(), class.len() as u64, 1)
    };
    let constructor_param_registered = unsafe {
        __elephc_eval_register_native_constructor_param(
            &mut ctx,
            class.as_ptr(),
            class.len() as u64,
            0,
            value.as_ptr(),
            value.len() as u64,
        )
    };
    let constructor_param_type_registered = unsafe {
        __elephc_eval_register_native_constructor_param_type(
            &mut ctx,
            class.as_ptr(),
            class.len() as u64,
            0,
            constructor_type.as_ptr(),
            constructor_type.len() as u64,
        )
    };
    let constructor_param_flags_registered = unsafe {
        __elephc_eval_register_native_constructor_param_flags(
            &mut ctx,
            class.as_ptr(),
            class.len() as u64,
            0,
            1,
            1,
        )
    };
    let constructor_bridge_support_registered = unsafe {
        __elephc_eval_register_native_constructor_bridge_support(
            &mut ctx,
            class.as_ptr(),
            class.len() as u64,
            0,
        )
    };
    let method_default_registered = unsafe {
        __elephc_eval_register_native_method_param_default_string(
            &mut ctx,
            method.as_ptr(),
            method.len() as u64,
            1,
            right.as_ptr(),
            right.len() as u64,
        )
    };
    let static_default_registered = unsafe {
        __elephc_eval_register_native_static_method_param_default_scalar(
            &mut ctx,
            static_method.as_ptr(),
            static_method.len() as u64,
            0,
            2,
            42,
        )
    };
    let static_empty_array_default_registered = unsafe {
        __elephc_eval_register_native_static_method_param_default_scalar(
            &mut ctx,
            static_method.as_ptr(),
            static_method.len() as u64,
            1,
            NATIVE_DEFAULT_EMPTY_ARRAY,
            0,
        )
    };
    let constructor_default_registered = unsafe {
        __elephc_eval_register_native_constructor_param_default_scalar(
            &mut ctx,
            class.as_ptr(),
            class.len() as u64,
            0,
            1,
            1,
        )
    };

    assert_eq!(method_registered, 1);
    assert_eq!(method_param_registered, 1);
    assert_eq!(method_param_type_registered, 1);
    assert_eq!(method_param_flags_registered, 1);
    assert_eq!(static_registered, 1);
    assert_eq!(static_param_registered, 1);
    assert_eq!(static_param_type_registered, 1);
    assert_eq!(static_param_flags_registered, 1);
    assert_eq!(static_return_type_registered, 1);
    assert_eq!(constructor_registered, 1);
    assert_eq!(constructor_param_registered, 1);
    assert_eq!(constructor_param_type_registered, 1);
    assert_eq!(constructor_param_flags_registered, 1);
    assert_eq!(constructor_bridge_support_registered, 1);
    assert_eq!(method_default_registered, 1);
    assert_eq!(static_default_registered, 1);
    assert_eq!(static_empty_array_default_registered, 1);
    assert_eq!(constructor_default_registered, 1);
    assert_eq!(
        ctx.native_method_signature("knownclass", "JOIN")
            .expect("method metadata")
            .param_names(),
        &["".to_string(), "right".to_string()]
    );
    let method_signature = ctx
        .native_method_signature("knownclass", "JOIN")
        .expect("method metadata");
    let method_type = method_signature
        .param_type(0)
        .expect("method parameter type");
    assert!(method_type.allows_null());
    assert_eq!(
        method_type.variants(),
        &[
            EvalParameterTypeVariant::Int,
            EvalParameterTypeVariant::String
        ]
    );
    assert!(method_signature.param_by_ref(1));
    assert!(!method_signature.param_variadic(1));
    assert_eq!(
        ctx.native_static_method_signature("KnownClass", "SUM")
            .expect("static method metadata")
            .param_names(),
        &["left".to_string(), "".to_string()]
    );
    let static_signature = ctx
        .native_static_method_signature("KnownClass", "SUM")
        .expect("static method metadata");
    let static_type = static_signature
        .param_type(0)
        .expect("static method parameter type");
    assert!(static_type.allows_null());
    assert_eq!(static_type.variants(), &[EvalParameterTypeVariant::String]);
    assert!(!static_signature.param_by_ref(1));
    assert!(static_signature.param_variadic(1));
    assert_eq!(
        static_signature
            .return_type()
            .expect("static return type")
            .variants(),
        &[EvalParameterTypeVariant::Bool]
    );
    assert_eq!(
        ctx.native_constructor_signature("knownclass")
            .expect("constructor metadata")
            .param_names(),
        &["value".to_string()]
    );
    let constructor_signature = ctx
        .native_constructor_signature("knownclass")
        .expect("constructor metadata");
    assert_eq!(
        constructor_signature
            .param_type(0)
            .expect("constructor parameter type")
            .variants(),
        &[EvalParameterTypeVariant::Class("KnownDep".to_string())]
    );
    assert!(constructor_signature.param_by_ref(0));
    assert!(constructor_signature.param_variadic(0));
    assert!(!constructor_signature.bridge_supported());
    assert_eq!(
        ctx.native_method_signature("knownclass", "JOIN")
            .expect("method metadata")
            .param_default(1),
        Some(&NativeCallableDefault::String("right".to_string()))
    );
    assert_eq!(
        ctx.native_static_method_signature("KnownClass", "SUM")
            .expect("static method metadata")
            .param_default(0),
        Some(&NativeCallableDefault::Int(42))
    );
    assert_eq!(
        ctx.native_static_method_signature("KnownClass", "SUM")
            .expect("static method metadata")
            .param_default(1),
        Some(&NativeCallableDefault::EmptyArray)
    );
    assert_eq!(
        ctx.native_constructor_signature("knownclass")
            .expect("constructor metadata")
            .param_default(0),
        Some(&NativeCallableDefault::Bool(true))
    );
}
