//! Purpose:
//! Tests eval ABI versioning, context state, call-site scopes, and declared
//! symbol visibility.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Context and symbol registration are exercised without generated assembly.

use super::*;

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
