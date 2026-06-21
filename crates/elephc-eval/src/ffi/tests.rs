//! Purpose:
//! Unit tests for the eval C ABI layer.
//! They validate handle allocation, stable status codes, scope flags, and
//! dynamic symbol registration without requiring generated runtime assembly.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - Execute remains a controlled unsupported stub in crate unit tests.
//! - Fake runtime-cell pointers are never dereferenced by these tests.

use super::context::*;
use super::execute::*;
use super::native_functions::*;
use super::native_methods::*;
use super::scope::*;
use super::symbols::*;
use crate::abi::{
    ElephcEvalContext, ElephcEvalResult, ElephcEvalScope, ABI_VERSION, SCOPE_FLAG_DIRTY,
    SCOPE_FLAG_OWNED, SCOPE_FLAG_PRESENT, SCOPE_FLAG_UNSET,
};
use crate::context::NativeCallableDefault;
use crate::errors::EvalStatus;
use crate::eval_ir::EvalParameterTypeVariant;
use crate::value::{RuntimeCell, RuntimeCellHandle};
use std::ffi::c_void;

/// Test native invoker placeholder used only to validate ABI registration.
unsafe extern "C" fn fake_native_invoker(
    _descriptor: *mut c_void,
    _args: *mut RuntimeCell,
) -> *mut RuntimeCell {
    std::ptr::null_mut()
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

/// Verifies native AOT registration records function and parameter metadata.
#[test]
fn register_native_function_reports_function_exists() {
    let mut ctx = ElephcEvalContext::new();
    let name = b"NATIVE_PROBE";
    let param = b"value";
    let descriptor = 1usize as *mut c_void;

    let registered = unsafe {
        __elephc_eval_register_native_function(
            &mut ctx,
            name.as_ptr(),
            name.len() as u64,
            descriptor,
            Some(fake_native_invoker),
            1,
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
    let exists = unsafe { __elephc_eval_function_exists(&ctx, b"native_probe".as_ptr(), 12) };

    assert_eq!(registered, 1);
    let native = ctx
        .native_function("native_probe")
        .expect("native function should be registered");

    assert_eq!(param_registered, 1);
    assert_eq!(exists, 1);
    assert_eq!(native.param_names(), &["value".to_string()]);
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
    assert_eq!(static_registered, 1);
    assert_eq!(static_param_registered, 1);
    assert_eq!(static_param_type_registered, 1);
    assert_eq!(static_return_type_registered, 1);
    assert_eq!(constructor_registered, 1);
    assert_eq!(constructor_param_registered, 1);
    assert_eq!(constructor_param_type_registered, 1);
    assert_eq!(method_default_registered, 1);
    assert_eq!(static_default_registered, 1);
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
        ctx.native_constructor_signature("knownclass")
            .expect("constructor metadata")
            .param_default(0),
        Some(&NativeCallableDefault::Bool(true))
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
