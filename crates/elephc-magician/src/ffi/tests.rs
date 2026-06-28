//! Purpose:
//! Unit tests for the eval C ABI layer.
//! They validate handle allocation, stable status codes, scope flags, and
//! dynamic symbol registration without requiring generated runtime assembly.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Execute remains a controlled unsupported stub in crate unit tests.
//! - Fake runtime-cell pointers are never dereferenced by these tests.

use super::context::*;
use super::declared_symbols::*;
use super::execute::*;
use super::native_functions::*;
use super::native_methods::*;
use super::scope::*;
use super::symbols::*;
use crate::abi::{
    ElephcEvalContext, ElephcEvalResult, ElephcEvalScope, ABI_VERSION, SCOPE_FLAG_DIRTY,
    SCOPE_FLAG_OWNED, SCOPE_FLAG_PRESENT, SCOPE_FLAG_UNSET,
};
use crate::context::{
    NativeCallableArrayDefaultElement, NativeCallableArrayDefaultKey, NativeCallableDefault,
    NativeCallableObjectDefaultArg, push_native_frame_called_class_override,
};
use crate::errors::EvalStatus;
use crate::eval_ir::{EvalAttributeArg, EvalParameterTypeVariant};
use crate::value::{RuntimeCell, RuntimeCellHandle};
use std::ffi::c_void;

const TEST_NATIVE_DEFAULT_NULL: u64 = 0;
const TEST_NATIVE_DEFAULT_BOOL: u64 = 1;
const TEST_NATIVE_DEFAULT_INT: u64 = 2;
const TEST_NATIVE_DEFAULT_FLOAT: u64 = 3;
const TEST_NATIVE_DEFAULT_EMPTY_ARRAY: u64 = 4;
const TEST_NATIVE_PROPERTY_REQUIRES_GET: u64 = 1;
const TEST_NATIVE_PROPERTY_REQUIRES_SET: u64 = 2;
const TEST_NATIVE_OBJECT_DEFAULT_ARG_SCALAR: u8 = 0;
const TEST_NATIVE_OBJECT_DEFAULT_ARG_STRING: u8 = 1;
const TEST_NATIVE_OBJECT_DEFAULT_ARG_OBJECT: u8 = 2;
const TEST_NATIVE_OBJECT_DEFAULT_ARG_NAMED: u8 = 3;
const TEST_NATIVE_OBJECT_DEFAULT_ARG_ARRAY: u8 = 4;
const TEST_NATIVE_ARRAY_DEFAULT_KEY_AUTO: u8 = 0;
const TEST_NATIVE_ARRAY_DEFAULT_KEY_INT: u8 = 1;
const TEST_NATIVE_ARRAY_DEFAULT_KEY_STRING: u8 = 2;
const TEST_MAX_NATIVE_OBJECT_DEFAULT_ARGS: usize = u8::MAX as usize;

/// Test native invoker placeholder used only to validate ABI registration.
unsafe extern "C" fn fake_native_invoker(
    _descriptor: *mut c_void,
    _args: *mut RuntimeCell,
) -> *mut RuntimeCell {
    std::ptr::null_mut()
}

/// Builds one native member-attribute ABI record for registration tests.
fn native_member_attribute_record(
    owner_kind: u8,
    member_key: &str,
    attribute_name: &str,
    args: Option<&[EvalAttributeArg]>,
) -> Vec<u8> {
    let mut record = Vec::new();
    record.push(owner_kind);
    native_member_attribute_push_string(&mut record, member_key);
    native_member_attribute_push_string(&mut record, attribute_name);
    match args {
        Some(args) => {
            record.push(1);
            record.extend_from_slice(&(args.len() as u32).to_le_bytes());
            for arg in args {
                native_member_attribute_push_arg(&mut record, arg);
            }
        }
        None => record.push(0),
    }
    record
}

/// Appends one test attribute argument to a native member-attribute ABI record.
fn native_member_attribute_push_arg(record: &mut Vec<u8>, arg: &EvalAttributeArg) {
    match arg {
        EvalAttributeArg::Null => record.push(0),
        EvalAttributeArg::Bool(value) => {
            record.push(1);
            record.push(u8::from(*value));
        }
        EvalAttributeArg::Int(value) => {
            record.push(2);
            record.extend_from_slice(&value.to_le_bytes());
        }
        EvalAttributeArg::Float(bits) => {
            record.push(5);
            record.extend_from_slice(&bits.to_le_bytes());
        }
        EvalAttributeArg::String(value) => {
            record.push(3);
            native_member_attribute_push_string(record, value);
        }
        EvalAttributeArg::Named { name, value } => {
            record.push(4);
            native_member_attribute_push_string(record, name);
            native_member_attribute_push_arg(record, value);
        }
        EvalAttributeArg::IntKeyed { .. } => {
            panic!("native attribute test ABI does not encode int-keyed array arguments")
        }
        EvalAttributeArg::Array(elements) => {
            record.push(6);
            record.extend_from_slice(&(elements.len() as u32).to_le_bytes());
            for element in elements {
                native_member_attribute_push_arg(record, element);
            }
        }
    }
}

/// Appends one length-prefixed string to a native member-attribute ABI record.
fn native_member_attribute_push_string(record: &mut Vec<u8>, value: &str) {
    record.extend_from_slice(&(value.len() as u32).to_le_bytes());
    record.extend_from_slice(value.as_bytes());
}

/// Builds one object-valued native parameter default ABI record for registration tests.
fn native_object_default_record(
    class_name: &str,
    args: &[NativeCallableObjectDefaultArg],
) -> Vec<u8> {
    let mut record = Vec::new();
    native_member_attribute_push_string(&mut record, class_name);
    record.push(args.len() as u8);
    for arg in args {
        native_object_default_push_arg(&mut record, arg);
    }
    record
}

/// Builds one array-valued native parameter default ABI record for registration tests.
fn native_array_default_record(elements: &[NativeCallableArrayDefaultElement]) -> Vec<u8> {
    let mut record = Vec::new();
    record.extend_from_slice(&(elements.len() as u32).to_le_bytes());
    for element in elements {
        native_array_default_push_element(&mut record, element);
    }
    record
}

/// Appends one array-default element and optional static key to a default record.
fn native_array_default_push_element(
    record: &mut Vec<u8>,
    element: &NativeCallableArrayDefaultElement,
) {
    match &element.key {
        Some(NativeCallableArrayDefaultKey::Int(value)) => {
            record.push(TEST_NATIVE_ARRAY_DEFAULT_KEY_INT);
            record.extend_from_slice(&value.to_le_bytes());
        }
        Some(NativeCallableArrayDefaultKey::String(value)) => {
            record.push(TEST_NATIVE_ARRAY_DEFAULT_KEY_STRING);
            native_member_attribute_push_string(record, value);
        }
        None => record.push(TEST_NATIVE_ARRAY_DEFAULT_KEY_AUTO),
    }
    native_object_default_push_arg_value(record, &element.value);
}

/// Appends one object-default constructor argument to a native parameter default record.
fn native_object_default_push_arg(record: &mut Vec<u8>, arg: &NativeCallableObjectDefaultArg) {
    if let Some(name) = &arg.name {
        record.push(TEST_NATIVE_OBJECT_DEFAULT_ARG_NAMED);
        native_member_attribute_push_string(record, name);
    }
    native_object_default_push_arg_value(record, &arg.value);
}

/// Appends one object-default constructor argument value to a native parameter default record.
fn native_object_default_push_arg_value(record: &mut Vec<u8>, value: &NativeCallableDefault) {
    match value {
        NativeCallableDefault::Null => {
            native_object_default_push_scalar(record, TEST_NATIVE_DEFAULT_NULL, 0)
        }
        NativeCallableDefault::Bool(value) => {
            native_object_default_push_scalar(record, TEST_NATIVE_DEFAULT_BOOL, u64::from(*value))
        }
        NativeCallableDefault::Int(value) => {
            native_object_default_push_scalar(record, TEST_NATIVE_DEFAULT_INT, *value as u64)
        }
        NativeCallableDefault::Float(value) => {
            native_object_default_push_scalar(record, TEST_NATIVE_DEFAULT_FLOAT, value.to_bits())
        }
        NativeCallableDefault::String(value) => {
            record.push(TEST_NATIVE_OBJECT_DEFAULT_ARG_STRING);
            native_member_attribute_push_string(record, value);
        }
        NativeCallableDefault::EmptyArray => {
            native_object_default_push_scalar(record, TEST_NATIVE_DEFAULT_EMPTY_ARRAY, 0)
        }
        NativeCallableDefault::Object { class_name, args } => {
            record.push(TEST_NATIVE_OBJECT_DEFAULT_ARG_OBJECT);
            record.extend_from_slice(&native_object_default_record(class_name, args));
        }
        NativeCallableDefault::Array(elements) => {
            record.push(TEST_NATIVE_OBJECT_DEFAULT_ARG_ARRAY);
            record.extend_from_slice(&native_array_default_record(elements));
        }
    }
}

/// Appends one scalar object-default constructor argument to a native parameter default record.
fn native_object_default_push_scalar(record: &mut Vec<u8>, kind: u64, payload: u64) {
    record.push(TEST_NATIVE_OBJECT_DEFAULT_ARG_SCALAR);
    record.extend_from_slice(&kind.to_le_bytes());
    record.extend_from_slice(&payload.to_le_bytes());
}

/// Verifies the exported version entry point reports the crate ABI constant.
#[test]
fn abi_version_matches_constant() {
    assert_eq!(__elephc_eval_abi_version(), ABI_VERSION);
}

/// Verifies the initial execute stub clears result storage and returns the
/// documented unsupported status instead of panicking or succeeding.
#[test]
fn execute_stub_returns_unsupported_and_clears_result() {
    let mut result = ElephcEvalResult {
        kind: 99,
        value_cell: 1usize as *mut std::ffi::c_void,
        error: 2usize as *mut std::ffi::c_void,
    };
    let status = unsafe {
        __elephc_eval_execute(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            b"$x = 1;".as_ptr(),
            7,
            &mut result,
        )
    };
    assert_eq!(status, EvalStatus::UnsupportedConstruct.code());
    assert_eq!(result.kind, 0);
    assert!(result.value_cell.is_null());
    assert!(result.error.is_null());
}

/// Verifies context allocation returns a current-version opaque handle.
#[test]
fn context_new_returns_current_version_handle() {
    let ctx = __elephc_eval_context_new();
    assert!(!ctx.is_null());
    let version = unsafe { (*ctx).abi_version() };
    unsafe {
        __elephc_eval_context_free(ctx);
    }
    assert_eq!(version, ABI_VERSION);
}

/// Verifies call-site metadata can be set through the stable context ABI.
#[test]
fn context_set_call_site_records_file_dir_and_line() {
    let mut ctx = ElephcEvalContext::new();
    let file = b"/tmp/source.php";
    let dir = b"/tmp";

    let status = unsafe {
        __elephc_eval_context_set_call_site(
            &mut ctx,
            file.as_ptr(),
            file.len() as u64,
            dir.as_ptr(),
            dir.len() as u64,
            9,
        )
    };

    assert_eq!(status, EvalStatus::Ok.code());
    assert_eq!(ctx.call_dir(), "/tmp");
    assert_eq!(ctx.eval_file_magic(), "/tmp/source.php(9) : eval()'d code");
}

/// Verifies the context ABI records a non-owned global scope handle.
#[test]
fn context_set_global_scope_records_handle() {
    let mut ctx = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();

    let status = unsafe { __elephc_eval_context_set_global_scope(&mut ctx, &mut scope) };

    assert_eq!(status, EvalStatus::Ok.code());
    assert_eq!(
        ctx.global_scope_ptr(),
        Some(&mut scope as *mut ElephcEvalScope)
    );
}

/// Verifies generated class scopes are pushed and popped through the context ABI.
#[test]
fn context_push_class_scope_records_self_and_called_class() {
    let mut ctx = ElephcEvalContext::new();
    let class_name = b"AotBase";
    let called_class_name = b"AotChild";

    let push_status = unsafe {
        __elephc_eval_context_push_class_scope(
            &mut ctx,
            class_name.as_ptr(),
            class_name.len() as u64,
            called_class_name.as_ptr(),
            called_class_name.len() as u64,
        )
    };

    assert_eq!(push_status, EvalStatus::Ok.code());
    assert_eq!(ctx.current_class_scope(), Some("AotBase"));
    assert_eq!(ctx.current_called_class_scope(), Some("AotChild"));

    let pop_status = unsafe { __elephc_eval_context_pop_class_scope(&mut ctx) };

    assert_eq!(pop_status, EvalStatus::Ok.code());
    assert_eq!(ctx.current_class_scope(), None);
    assert_eq!(ctx.current_called_class_scope(), None);
}

/// Verifies generated frames can query eval late-static overrides without a context handle.
#[test]
fn native_frame_called_class_override_reports_thread_local_scope() {
    let class_name = b"AotBase";
    let mut out_ptr = std::ptr::null();
    let mut out_len = 0;

    let missing = unsafe {
        __elephc_eval_native_frame_called_class_override(
            class_name.as_ptr(),
            class_name.len() as u64,
            &mut out_ptr,
            &mut out_len,
        )
    };

    assert_eq!(missing, 0);
    assert!(out_ptr.is_null());
    assert_eq!(out_len, 0);

    {
        let _guard = push_native_frame_called_class_override(
            std::ptr::null_mut(),
            "AotBase",
            "EvalChild",
        );

        let found = unsafe {
            __elephc_eval_native_frame_called_class_override(
                class_name.as_ptr(),
                class_name.len() as u64,
                &mut out_ptr,
                &mut out_len,
            )
        };

        assert_eq!(found, 1);
        let bytes = unsafe { std::slice::from_raw_parts(out_ptr, out_len as usize) };
        assert_eq!(bytes, b"EvalChild");
    }

    let after_drop = unsafe {
        __elephc_eval_native_frame_called_class_override(
            class_name.as_ptr(),
            class_name.len() as u64,
            &mut out_ptr,
            &mut out_len,
        )
    };

    assert_eq!(after_drop, 0);
    assert!(out_ptr.is_null());
    assert_eq!(out_len, 0);
}

/// Verifies generated declaration-name metadata is exposed through eval lists.
#[test]
fn register_declared_symbol_names_records_visible_metadata() {
    let mut ctx = ElephcEvalContext::new();
    let class_name = b"\\AotDeclaredClass";
    let class_duplicate = b"aotdeclaredclass";
    let interface_name = b"AotDeclaredInterface";
    let trait_name = b"AotDeclaredTrait";
    let empty_name = b"";

    let class_registered = unsafe {
        __elephc_eval_register_declared_class_name(
            &mut ctx,
            class_name.as_ptr(),
            class_name.len() as u64,
        )
    };
    let duplicate_registered = unsafe {
        __elephc_eval_register_declared_class_name(
            &mut ctx,
            class_duplicate.as_ptr(),
            class_duplicate.len() as u64,
        )
    };
    let interface_registered = unsafe {
        __elephc_eval_register_declared_interface_name(
            &mut ctx,
            interface_name.as_ptr(),
            interface_name.len() as u64,
        )
    };
    let trait_registered = unsafe {
        __elephc_eval_register_declared_trait_name(
            &mut ctx,
            trait_name.as_ptr(),
            trait_name.len() as u64,
        )
    };
    let empty_rejected = unsafe {
        __elephc_eval_register_declared_trait_name(
            &mut ctx,
            empty_name.as_ptr(),
            empty_name.len() as u64,
        )
    };

    assert_eq!(class_registered, 1);
    assert_eq!(duplicate_registered, 1);
    assert_eq!(interface_registered, 1);
    assert_eq!(trait_registered, 1);
    assert_eq!(empty_rejected, 0);
    assert_eq!(ctx.declared_class_names(), &["AotDeclaredClass".to_string()]);
    assert_eq!(
        ctx.declared_interface_names(),
        &["AotDeclaredInterface".to_string()]
    );
    assert_eq!(ctx.declared_trait_names(), &["AotDeclaredTrait".to_string()]);
}

/// Verifies the function-exists ABI probes eval-declared functions by folded name.
#[test]
fn function_exists_reports_declared_eval_function() {
    let mut ctx = ElephcEvalContext::new();
    ctx.define_function(
        "dyn_probe",
        crate::eval_ir::EvalFunction::new("dyn_probe", Vec::new(), Vec::new()),
    )
    .expect("first dynamic function declaration should succeed");
    let existing = b"DYN_PROBE";
    let missing = b"missing";

    let existing_result =
        unsafe { __elephc_eval_function_exists(&ctx, existing.as_ptr(), existing.len() as u64) };
    let missing_result =
        unsafe { __elephc_eval_function_exists(&ctx, missing.as_ptr(), missing.len() as u64) };

    assert_eq!(existing_result, 1);
    assert_eq!(missing_result, 0);
}

/// Verifies the constant-exists ABI probes eval-defined constants by PHP name.
#[test]
fn constant_exists_reports_defined_eval_constant() {
    let mut ctx = ElephcEvalContext::new();
    let value = RuntimeCellHandle::from_raw(1usize as *mut RuntimeCell);
    assert!(ctx.define_constant("DynConstProbe", value));
    let existing = b"DynConstProbe";
    let qualified = b"\\DynConstProbe";
    let wrong_case = b"dynconstprobe";
    let missing = b"missing";

    let existing_result =
        unsafe { __elephc_eval_constant_exists(&ctx, existing.as_ptr(), existing.len() as u64) };
    let qualified_result =
        unsafe { __elephc_eval_constant_exists(&ctx, qualified.as_ptr(), qualified.len() as u64) };
    let wrong_case_result = unsafe {
        __elephc_eval_constant_exists(&ctx, wrong_case.as_ptr(), wrong_case.len() as u64)
    };
    let missing_result =
        unsafe { __elephc_eval_constant_exists(&ctx, missing.as_ptr(), missing.len() as u64) };

    assert_eq!(existing_result, 1);
    assert_eq!(qualified_result, 1);
    assert_eq!(wrong_case_result, 0);
    assert_eq!(missing_result, 0);
}

/// Verifies the dynamic-class-exists ABI probes eval-declared classes by folded PHP name.
#[test]
fn dynamic_class_exists_reports_declared_eval_class() {
    let mut ctx = ElephcEvalContext::new();
    assert!(ctx.define_class(crate::eval_ir::EvalClass::new(
        "DynClassProbe",
        Vec::new(),
        Vec::new()
    )));
    let existing = b"DynClassProbe";
    let qualified = b"\\DynClassProbe";
    let folded = b"dynclassprobe";
    let missing = b"missing";

    let existing_result = unsafe {
        __elephc_eval_dynamic_class_exists(&ctx, existing.as_ptr(), existing.len() as u64)
    };
    let qualified_result = unsafe {
        __elephc_eval_dynamic_class_exists(&ctx, qualified.as_ptr(), qualified.len() as u64)
    };
    let folded_result =
        unsafe { __elephc_eval_dynamic_class_exists(&ctx, folded.as_ptr(), folded.len() as u64) };
    let missing_result =
        unsafe { __elephc_eval_dynamic_class_exists(&ctx, missing.as_ptr(), missing.len() as u64) };

    assert_eq!(existing_result, 1);
    assert_eq!(qualified_result, 1);
    assert_eq!(folded_result, 1);
    assert_eq!(missing_result, 0);
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

/// Verifies native AOT object defaults can carry nested object constructor args.
#[test]
fn register_native_object_default_decodes_nested_objects() {
    let mut ctx = ElephcEvalContext::new();
    let class = b"KnownClass";
    let nested_args = vec![NativeCallableObjectDefaultArg::positional(
        NativeCallableDefault::Object {
            class_name: "InnerDefault".to_string(),
            args: vec![NativeCallableObjectDefaultArg::positional(
                NativeCallableDefault::String("leaf".to_string()),
            )],
        },
    )];
    let expected_default = NativeCallableDefault::Object {
        class_name: "OuterDefault".to_string(),
        args: nested_args.clone(),
    };
    let spec = native_object_default_record("OuterDefault", &nested_args);

    let constructor_registered = unsafe {
        __elephc_eval_register_native_constructor(&mut ctx, class.as_ptr(), class.len() as u64, 1)
    };
    let default_registered = unsafe {
        __elephc_eval_register_native_constructor_param_default_object(
            &mut ctx,
            class.as_ptr(),
            class.len() as u64,
            0,
            spec.as_ptr(),
            spec.len() as u64,
        )
    };

    assert_eq!(constructor_registered, 1);
    assert_eq!(default_registered, 1);
    assert_eq!(
        ctx.native_constructor_signature("knownclass")
            .expect("constructor metadata")
            .param_default(0),
        Some(&expected_default)
    );
}

/// Verifies native AOT object defaults can carry named constructor args.
#[test]
fn register_native_object_default_decodes_named_args() {
    let mut ctx = ElephcEvalContext::new();
    let class = b"KnownClass";
    let args = vec![
        NativeCallableObjectDefaultArg::named(
            "right",
            NativeCallableDefault::String("R".to_string()),
        ),
        NativeCallableObjectDefaultArg::named(
            "left",
            NativeCallableDefault::String("L".to_string()),
        ),
    ];
    let expected_default = NativeCallableDefault::Object {
        class_name: "NamedDefault".to_string(),
        args: args.clone(),
    };
    let spec = native_object_default_record("NamedDefault", &args);

    let constructor_registered = unsafe {
        __elephc_eval_register_native_constructor(&mut ctx, class.as_ptr(), class.len() as u64, 1)
    };
    let default_registered = unsafe {
        __elephc_eval_register_native_constructor_param_default_object(
            &mut ctx,
            class.as_ptr(),
            class.len() as u64,
            0,
            spec.as_ptr(),
            spec.len() as u64,
        )
    };

    assert_eq!(constructor_registered, 1);
    assert_eq!(default_registered, 1);
    assert_eq!(
        ctx.native_constructor_signature("knownclass")
            .expect("constructor metadata")
            .param_default(0),
        Some(&expected_default)
    );
}

/// Verifies native AOT object defaults decode the full u8 constructor argument range.
#[test]
fn register_native_object_default_decodes_full_u8_arg_count() {
    let mut ctx = ElephcEvalContext::new();
    let class = b"KnownClass";
    let args = (0..TEST_MAX_NATIVE_OBJECT_DEFAULT_ARGS)
        .map(|index| {
            let value = NativeCallableDefault::String(format!("arg{}", index));
            NativeCallableObjectDefaultArg::positional(value)
        })
        .collect::<Vec<_>>();
    let expected_default = NativeCallableDefault::Object {
        class_name: "LargeDefault".to_string(),
        args: args.clone(),
    };
    let spec = native_object_default_record("LargeDefault", &args);

    let constructor_registered = unsafe {
        __elephc_eval_register_native_constructor(&mut ctx, class.as_ptr(), class.len() as u64, 1)
    };
    let default_registered = unsafe {
        __elephc_eval_register_native_constructor_param_default_object(
            &mut ctx,
            class.as_ptr(),
            class.len() as u64,
            0,
            spec.as_ptr(),
            spec.len() as u64,
        )
    };

    assert_eq!(constructor_registered, 1);
    assert_eq!(default_registered, 1);
    assert_eq!(
        ctx.native_constructor_signature("knownclass")
            .expect("constructor metadata")
            .param_default(0),
        Some(&expected_default)
    );
}

/// Verifies native AOT array defaults can carry keyed and nested default values.
#[test]
fn register_native_array_default_decodes_nested_values() {
    let mut ctx = ElephcEvalContext::new();
    let method = b"KnownClass::items";
    let class = b"KnownClass";
    let property = b"KnownClass::items";
    let elements = vec![
        NativeCallableArrayDefaultElement::keyed(
            NativeCallableArrayDefaultKey::String("left".to_string()),
            NativeCallableDefault::String("L".to_string()),
        ),
        NativeCallableArrayDefaultElement::keyed(
            NativeCallableArrayDefaultKey::Int(2),
            NativeCallableDefault::Array(vec![NativeCallableArrayDefaultElement::positional(
                NativeCallableDefault::Int(7),
            )]),
        ),
        NativeCallableArrayDefaultElement::positional(NativeCallableDefault::Object {
            class_name: "ArrayDefaultDep".to_string(),
            args: vec![NativeCallableObjectDefaultArg::named(
                "label",
                NativeCallableDefault::String("dep".to_string()),
            )],
        }),
    ];
    let expected_default = NativeCallableDefault::Array(elements.clone());
    let spec = native_array_default_record(&elements);

    let method_registered = unsafe {
        __elephc_eval_register_native_method(&mut ctx, method.as_ptr(), method.len() as u64, 1)
    };
    let method_default_registered = unsafe {
        __elephc_eval_register_native_method_param_default_array(
            &mut ctx,
            method.as_ptr(),
            method.len() as u64,
            0,
            spec.as_ptr(),
            spec.len() as u64,
        )
    };
    let constructor_registered = unsafe {
        __elephc_eval_register_native_constructor(&mut ctx, class.as_ptr(), class.len() as u64, 1)
    };
    let constructor_default_registered = unsafe {
        __elephc_eval_register_native_constructor_param_default_array(
            &mut ctx,
            class.as_ptr(),
            class.len() as u64,
            0,
            spec.as_ptr(),
            spec.len() as u64,
        )
    };
    let property_default_registered = unsafe {
        __elephc_eval_register_native_property_default_array(
            &mut ctx,
            property.as_ptr(),
            property.len() as u64,
            spec.as_ptr(),
            spec.len() as u64,
        )
    };

    assert_eq!(method_registered, 1);
    assert_eq!(method_default_registered, 1);
    assert_eq!(constructor_registered, 1);
    assert_eq!(constructor_default_registered, 1);
    assert_eq!(property_default_registered, 1);
    assert_eq!(
        ctx.native_method_signature("knownclass", "ITEMS")
            .expect("method metadata")
            .param_default(0),
        Some(&expected_default)
    );
    assert_eq!(
        ctx.native_constructor_signature("knownclass")
            .expect("constructor metadata")
            .param_default(0),
        Some(&expected_default)
    );
    assert_eq!(
        ctx.native_property_default("KnownClass", "items"),
        Some(expected_default)
    );
}

/// Verifies native AOT parent metadata is available for eval static-scope resolution.
#[test]
fn register_native_class_parent_records_metadata() {
    let mut ctx = ElephcEvalContext::new();
    let class = b"KnownChild";
    let parent = b"KnownParent";

    let registered = unsafe {
        __elephc_eval_register_native_class_parent(
            &mut ctx,
            class.as_ptr(),
            class.len() as u64,
            parent.as_ptr(),
            parent.len() as u64,
        )
    };

    assert_eq!(registered, 1);
    assert_eq!(ctx.native_class_parent("knownchild"), Some("KnownParent"));
}

/// Verifies native AOT property type metadata is available to eval reflection.
#[test]
fn register_native_property_type_records_metadata() {
    let mut ctx = ElephcEvalContext::new();
    let property = b"KnownClass::name";
    let property_type = b"?KnownDep";
    let invalid_property = b"KnownClass::bad";
    let invalid_type = b"void";

    let registered = unsafe {
        __elephc_eval_register_native_property_type(
            &mut ctx,
            property.as_ptr(),
            property.len() as u64,
            property_type.as_ptr(),
            property_type.len() as u64,
        )
    };
    let invalid_registered = unsafe {
        __elephc_eval_register_native_property_type(
            &mut ctx,
            invalid_property.as_ptr(),
            invalid_property.len() as u64,
            invalid_type.as_ptr(),
            invalid_type.len() as u64,
        )
    };

    assert_eq!(registered, 1);
    let property_type = ctx
        .native_property_type("knownclass", "name")
        .expect("property type metadata");
    assert!(property_type.allows_null());
    assert_eq!(
        property_type.variants(),
        &[EvalParameterTypeVariant::Class("KnownDep".to_string())]
    );
    assert_eq!(invalid_registered, 0);
    assert!(ctx.native_property_type("KnownClass", "bad").is_none());
}

/// Verifies native AOT interface property contracts are available to eval validation.
#[test]
fn register_native_interface_property_records_metadata() {
    let mut ctx = ElephcEvalContext::new();
    let property = b"KnownContract::KnownParentContract::name";
    let property_type = b"string";
    let invalid_property = b"KnownContract::KnownParentContract::bad";
    let invalid_type = b"void";

    let registered = unsafe {
        __elephc_eval_register_native_interface_property(
            &mut ctx,
            property.as_ptr(),
            property.len() as u64,
            property_type.as_ptr(),
            property_type.len() as u64,
            TEST_NATIVE_PROPERTY_REQUIRES_GET | TEST_NATIVE_PROPERTY_REQUIRES_SET,
        )
    };
    let invalid_registered = unsafe {
        __elephc_eval_register_native_interface_property(
            &mut ctx,
            invalid_property.as_ptr(),
            invalid_property.len() as u64,
            invalid_type.as_ptr(),
            invalid_type.len() as u64,
            TEST_NATIVE_PROPERTY_REQUIRES_GET,
        )
    };

    let requirements = ctx.native_interface_property_requirements("knowncontract");
    assert_eq!(registered, 1);
    assert_eq!(requirements.len(), 1);
    assert_eq!(requirements[0].0, "KnownParentContract");
    assert_eq!(requirements[0].1.name(), "name");
    assert!(requirements[0].1.requires_get());
    assert!(requirements[0].1.requires_set());
    assert_eq!(
        requirements[0].1.property_type().map(|ty| ty.variants()),
        Some(&[EvalParameterTypeVariant::String][..])
    );
    assert_eq!(invalid_registered, 0);
    assert!(ctx
        .native_interface_property_requirements("KnownContract")
        .iter()
        .all(|(_, property)| property.name() != "bad"));
}

/// Verifies native AOT abstract class property contracts are available to eval validation.
#[test]
fn register_native_abstract_property_records_metadata() {
    let mut ctx = ElephcEvalContext::new();
    let property = b"KnownClass::KnownParent::name";
    let property_type = b"string";
    let invalid_property = b"KnownClass::KnownParent::bad";
    let invalid_type = b"void";

    let registered = unsafe {
        __elephc_eval_register_native_abstract_property(
            &mut ctx,
            property.as_ptr(),
            property.len() as u64,
            property_type.as_ptr(),
            property_type.len() as u64,
            TEST_NATIVE_PROPERTY_REQUIRES_GET | TEST_NATIVE_PROPERTY_REQUIRES_SET,
        )
    };
    let invalid_registered = unsafe {
        __elephc_eval_register_native_abstract_property(
            &mut ctx,
            invalid_property.as_ptr(),
            invalid_property.len() as u64,
            invalid_type.as_ptr(),
            invalid_type.len() as u64,
            TEST_NATIVE_PROPERTY_REQUIRES_GET,
        )
    };

    let requirements = ctx.native_abstract_property_requirements("knownclass");
    assert_eq!(registered, 1);
    assert_eq!(requirements.len(), 1);
    assert_eq!(requirements[0].0, "KnownParent");
    assert_eq!(requirements[0].1.name(), "name");
    assert!(requirements[0].1.requires_get());
    assert!(requirements[0].1.requires_set());
    assert_eq!(
        requirements[0].1.property_type().map(|ty| ty.variants()),
        Some(&[EvalParameterTypeVariant::String][..])
    );
    assert_eq!(invalid_registered, 0);
    assert!(ctx
        .native_abstract_property_requirements("KnownClass")
        .iter()
        .all(|(_, property)| property.name() != "bad"));
}

/// Verifies native AOT property default metadata is available to eval reflection.
#[test]
fn register_native_property_default_records_metadata() {
    let mut ctx = ElephcEvalContext::new();
    let count = b"KnownClass::count";
    let label = b"KnownClass::label";
    let invalid = b"KnownClass::invalid";
    let label_value = b"ok";

    let scalar_registered = unsafe {
        __elephc_eval_register_native_property_default_scalar(
            &mut ctx,
            count.as_ptr(),
            count.len() as u64,
            2,
            42,
        )
    };
    let string_registered = unsafe {
        __elephc_eval_register_native_property_default_string(
            &mut ctx,
            label.as_ptr(),
            label.len() as u64,
            label_value.as_ptr(),
            label_value.len() as u64,
        )
    };
    let invalid_registered = unsafe {
        __elephc_eval_register_native_property_default_scalar(
            &mut ctx,
            invalid.as_ptr(),
            invalid.len() as u64,
            99,
            0,
        )
    };

    assert_eq!(scalar_registered, 1);
    assert_eq!(
        ctx.native_property_default("knownclass", "count"),
        Some(NativeCallableDefault::Int(42))
    );
    assert_eq!(string_registered, 1);
    assert_eq!(
        ctx.native_property_default("KnownClass", "label"),
        Some(NativeCallableDefault::String("ok".to_string()))
    );
    assert_eq!(invalid_registered, 0);
    assert!(ctx
        .native_property_default("KnownClass", "invalid")
        .is_none());
}

/// Verifies native AOT member attributes are available to eval reflection.
#[test]
fn register_native_member_attribute_records_metadata() {
    let mut ctx = ElephcEvalContext::new();
    let method_record = native_member_attribute_record(
        0,
        "KnownClass::run",
        "Route",
        Some(&[
            EvalAttributeArg::String("api".to_string()),
            EvalAttributeArg::Named {
                name: "path".to_string(),
                value: Box::new(EvalAttributeArg::String("/users".to_string())),
            },
            EvalAttributeArg::Int(7),
            EvalAttributeArg::Float(1.5f64.to_bits()),
            EvalAttributeArg::Array(vec![
                EvalAttributeArg::Int(1),
                EvalAttributeArg::String("two".to_string()),
            ]),
            EvalAttributeArg::Bool(true),
            EvalAttributeArg::Null,
        ]),
    );
    let property_record = native_member_attribute_record(1, "KnownClass::id", "Column", None);
    let constant_record = native_member_attribute_record(
        2,
        "KnownClass::LIMIT",
        "Limit",
        Some(&[EvalAttributeArg::Int(100)]),
    );
    let class_record = native_member_attribute_record(
        3,
        "KnownClass",
        "Entity",
        Some(&[EvalAttributeArg::String("model".to_string())]),
    );
    let invalid_record = [99, 0, 0, 0, 0];

    let method_registered = unsafe {
        __elephc_eval_register_native_member_attribute(
            &mut ctx,
            method_record.as_ptr(),
            method_record.len() as u64,
        )
    };
    let property_registered = unsafe {
        __elephc_eval_register_native_member_attribute(
            &mut ctx,
            property_record.as_ptr(),
            property_record.len() as u64,
        )
    };
    let constant_registered = unsafe {
        __elephc_eval_register_native_member_attribute(
            &mut ctx,
            constant_record.as_ptr(),
            constant_record.len() as u64,
        )
    };
    let class_registered = unsafe {
        __elephc_eval_register_native_member_attribute(
            &mut ctx,
            class_record.as_ptr(),
            class_record.len() as u64,
        )
    };
    let invalid_registered = unsafe {
        __elephc_eval_register_native_member_attribute(
            &mut ctx,
            invalid_record.as_ptr(),
            invalid_record.len() as u64,
        )
    };

    assert_eq!(method_registered, 1);
    let method_attributes = ctx.native_method_attributes("knownclass", "RUN");
    assert_eq!(method_attributes.len(), 1);
    assert_eq!(method_attributes[0].name(), "Route");
    assert_eq!(
        method_attributes[0].args(),
        Some(
            [
                EvalAttributeArg::String("api".to_string()),
                EvalAttributeArg::Named {
                    name: "path".to_string(),
                    value: Box::new(EvalAttributeArg::String("/users".to_string())),
                },
                EvalAttributeArg::Int(7),
                EvalAttributeArg::Float(1.5f64.to_bits()),
                EvalAttributeArg::Array(vec![
                    EvalAttributeArg::Int(1),
                    EvalAttributeArg::String("two".to_string()),
                ]),
                EvalAttributeArg::Bool(true),
                EvalAttributeArg::Null,
            ]
            .as_slice()
        )
    );
    assert_eq!(property_registered, 1);
    let property_attributes = ctx.native_property_attributes("KnownClass", "id");
    assert_eq!(property_attributes.len(), 1);
    assert_eq!(property_attributes[0].name(), "Column");
    assert!(property_attributes[0].args().is_none());
    assert_eq!(constant_registered, 1);
    let constant_attributes = ctx.native_constant_attributes("KnownClass", "LIMIT");
    assert_eq!(constant_attributes.len(), 1);
    assert_eq!(constant_attributes[0].name(), "Limit");
    assert_eq!(
        constant_attributes[0].args(),
        Some([EvalAttributeArg::Int(100)].as_slice())
    );
    assert_eq!(class_registered, 1);
    let class_attributes = ctx.native_class_attributes("knownclass");
    assert_eq!(class_attributes.len(), 1);
    assert_eq!(class_attributes[0].name(), "Entity");
    assert_eq!(
        class_attributes[0].args(),
        Some([EvalAttributeArg::String("model".to_string())].as_slice())
    );
    assert_eq!(invalid_registered, 0);
}

/// Verifies scope allocation returns an empty opaque activation scope handle.
#[test]
fn scope_new_returns_empty_handle() {
    let scope = __elephc_eval_scope_new();
    assert!(!scope.is_null());
    let generation = unsafe { (*scope).generation() };
    unsafe {
        __elephc_eval_scope_free(scope);
    }
    assert_eq!(generation, 0);
}

/// Verifies execute rejects contexts whose ABI version no longer matches.
#[test]
fn execute_rejects_mismatched_context_version() {
    let mut ctx = ElephcEvalContext::for_abi_version(ABI_VERSION + 1);
    let status = unsafe {
        __elephc_eval_execute(
            &mut ctx,
            std::ptr::null_mut(),
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
        )
    };

    assert_eq!(status, EvalStatus::AbiMismatch.code());
}

/// Verifies execute maps invalid eval fragments to the stable parse status.
#[test]
fn execute_rejects_php_opening_tags_as_parse_errors() {
    let code = b"<?php echo 1;";
    let status = unsafe {
        __elephc_eval_execute(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            code.as_ptr(),
            code.len() as u64,
            std::ptr::null_mut(),
        )
    };

    assert_eq!(status, EvalStatus::ParseError.code());
}

/// Verifies execute maps invalid ABI code storage to runtime fatal instead of panicking.
#[test]
fn execute_rejects_null_code_pointer_with_nonzero_length() {
    let status = unsafe {
        __elephc_eval_execute(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
            1,
            std::ptr::null_mut(),
        )
    };

    assert_eq!(status, EvalStatus::RuntimeFatal.code());
}

/// Verifies scope set/get expose runtime-cell handles and dirty flags through the ABI.
#[test]
fn scope_set_get_round_trips_cell_and_flags() {
    let scope = __elephc_eval_scope_new();
    let name = b"x";
    let cell = 1usize as *mut RuntimeCell;
    let set_status = unsafe {
        __elephc_eval_scope_set(
            scope,
            name.as_ptr(),
            name.len() as u64,
            cell,
            SCOPE_FLAG_OWNED,
        )
    };
    let mut out_cell = std::ptr::null_mut();
    let mut out_flags = 0;
    let get_status = unsafe {
        __elephc_eval_scope_get(
            scope,
            name.as_ptr(),
            name.len() as u64,
            &mut out_cell,
            &mut out_flags,
        )
    };
    unsafe {
        __elephc_eval_scope_free(scope);
    }

    assert_eq!(set_status, EvalStatus::Ok.code());
    assert_eq!(get_status, EvalStatus::Ok.code());
    assert_eq!(out_cell, cell);
    assert_eq!(out_flags & SCOPE_FLAG_PRESENT, SCOPE_FLAG_PRESENT);
    assert_eq!(out_flags & SCOPE_FLAG_DIRTY, SCOPE_FLAG_DIRTY);
    assert_eq!(out_flags & SCOPE_FLAG_OWNED, SCOPE_FLAG_OWNED);
}

/// Verifies the alias ABI maps a local eval variable to a global name.
#[test]
fn scope_mark_global_alias_records_target_name() {
    let scope = __elephc_eval_scope_new();
    let name = b"alias";
    let global_name = b"source";

    let status = unsafe {
        __elephc_eval_scope_mark_global_alias(
            scope,
            name.as_ptr(),
            name.len() as u64,
            global_name.as_ptr(),
            global_name.len() as u64,
        )
    };
    let target = unsafe { (*scope).global_alias_target("alias").map(str::to_string) };
    unsafe {
        __elephc_eval_scope_free(scope);
    }

    assert_eq!(status, EvalStatus::Ok.code());
    assert_eq!(target.as_deref(), Some("source"));
}

/// Verifies scope unset and clear-dirty expose missing/clean state through the ABI.
#[test]
fn scope_unset_and_clear_dirty_update_flags() {
    let scope = __elephc_eval_scope_new();
    let name = b"x";
    let cell = 1usize as *mut RuntimeCell;
    unsafe {
        __elephc_eval_scope_set(
            scope,
            name.as_ptr(),
            name.len() as u64,
            cell,
            SCOPE_FLAG_OWNED,
        );
        __elephc_eval_scope_clear_dirty(scope);
        __elephc_eval_scope_unset(scope, name.as_ptr(), name.len() as u64);
    }
    let mut out_cell = cell;
    let mut out_flags = 0;
    let get_status = unsafe {
        __elephc_eval_scope_get(
            scope,
            name.as_ptr(),
            name.len() as u64,
            &mut out_cell,
            &mut out_flags,
        )
    };
    unsafe {
        __elephc_eval_scope_free(scope);
    }

    assert_eq!(get_status, EvalStatus::Ok.code());
    assert!(out_cell.is_null());
    assert_eq!(out_flags & SCOPE_FLAG_UNSET, SCOPE_FLAG_UNSET);
    assert_eq!(out_flags & SCOPE_FLAG_DIRTY, SCOPE_FLAG_DIRTY);
    assert_eq!(out_flags & SCOPE_FLAG_PRESENT, 0);
}
