//! Purpose:
//! Exports eval context handle allocation and context metadata setters.
//! These functions manage process-level eval state and call-site/global/class
//! scope metadata used while executing fragments.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_context_*` symbols.
//!
//! Key details:
//! - Context handles are opaque across the ABI.
//! - Call-site metadata is UTF-8 and is validated before storing.

use super::util::abi_name_to_string;
use crate::abi::{ElephcEvalContext, ElephcEvalScope, ABI_VERSION};
use crate::context::native_frame_called_class_override_bytes;
use crate::errors::EvalStatus;
#[cfg(not(test))]
use crate::ffi::dynamic_destructors::install_dynamic_object_destructor_hook;
use std::ptr;

/// Returns the ABI version expected by generated elephc eval call sites.
#[no_mangle]
pub extern "C" fn __elephc_eval_abi_version() -> u32 {
    ABI_VERSION
}

/// Allocates a process-level eval context handle for generated code.
#[no_mangle]
pub extern "C" fn __elephc_eval_context_new() -> *mut ElephcEvalContext {
    #[cfg(not(test))]
    install_dynamic_object_destructor_hook();
    Box::into_raw(Box::new(ElephcEvalContext::new()))
}

/// Marks this program's eval bridge as strict-PHP: extension builtins
/// (`ptr_*`, `buffer_*`, `class_attribute_*`) disappear from eval dispatch and
/// introspection, matching the PHP interpreter where those names do not exist.
///
/// Generated code emits this call while initializing the eval context, only in
/// binaries compiled with `elephc --strict-php`. The flag is thread-local and
/// elephc programs run every eval on the initializing thread, so one call
/// covers the program lifetime.
#[no_mangle]
pub extern "C" fn __elephc_eval_set_strict_php(enabled: u8) {
    crate::strict_php_mode::set_strict_php_mode(enabled != 0);
}

/// Frees a process-level eval context handle allocated by the eval bridge.
///
/// # Safety
/// `ctx` must be null or a pointer returned by `__elephc_eval_context_new`
/// that has not already been freed.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_context_free(ctx: *mut ElephcEvalContext) {
    if !ctx.is_null() {
        if let Some(context) = unsafe { ctx.as_ref() } {
            context.unregister_dynamic_object_context();
        }
        drop(Box::from_raw(ctx));
    }
}

/// Records source metadata for the next eval fragment executed in this context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `file_ptr` and `dir_ptr` must be
/// readable for their matching lengths when the length is greater than zero.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_context_set_call_site(
    ctx: *mut ElephcEvalContext,
    file_ptr: *const u8,
    file_len: u64,
    dir_ptr: *const u8,
    dir_len: u64,
    line: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_context_set_call_site_inner(ctx, file_ptr, file_len, dir_ptr, dir_len, line)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Records the materialized program-global eval scope for `global` aliases.
///
/// # Safety
/// `ctx` and `scope` must be valid handles allocated by the eval bridge. The
/// context does not own `scope`; generated code must keep the scope alive for
/// as long as the context can execute eval fragments that reference globals.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_context_set_global_scope(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
) -> i32 {
    std::panic::catch_unwind(|| unsafe { eval_context_set_global_scope_inner(ctx, scope) })
        .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Enters a generated caller's class scope for the next eval fragment.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Class name pointers must be
/// readable UTF-8 slices for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_context_push_class_scope(
    ctx: *mut ElephcEvalContext,
    class_ptr: *const u8,
    class_len: u64,
    called_class_ptr: *const u8,
    called_class_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_context_push_class_scope_inner(
            ctx,
            class_ptr,
            class_len,
            called_class_ptr,
            called_class_len,
        )
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Leaves a generated caller class scope after an eval fragment returns.
///
/// # Safety
/// `ctx` must be a valid eval context handle previously passed to
/// `__elephc_eval_context_push_class_scope`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_context_pop_class_scope(ctx: *mut ElephcEvalContext) -> i32 {
    std::panic::catch_unwind(|| unsafe { eval_context_pop_class_scope_inner(ctx) })
        .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Reads the late-static override currently installed for a generated/AOT frame.
///
/// # Safety
/// `class_ptr` must be a readable UTF-8 slice for `class_len` bytes. `out_ptr`
/// and `out_len` must be valid writable out-parameters. Returned bytes are
/// owned by eval thread-local state and remain valid until the native frame
/// override guard is dropped.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_native_frame_called_class_override(
    class_ptr: *const u8,
    class_len: u64,
    out_ptr: *mut *const u8,
    out_len: *mut u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_native_frame_called_class_override_inner(class_ptr, class_len, out_ptr, out_len)
    })
    .unwrap_or(0)
}

/// Runs the call-site metadata setter ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_context_set_call_site`; callers must pass a valid
/// context and readable UTF-8 file/directory byte slices.
unsafe fn eval_context_set_call_site_inner(
    ctx: *mut ElephcEvalContext,
    file_ptr: *const u8,
    file_len: u64,
    dir_ptr: *const u8,
    dir_len: u64,
    line: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    let Ok(file) = abi_name_to_string(file_ptr, file_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(dir) = abi_name_to_string(dir_ptr, dir_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(line) = i64::try_from(line) else {
        return EvalStatus::RuntimeFatal.code();
    };
    context.set_call_site(file, dir, line);
    EvalStatus::Ok.code()
}

/// Runs the global-scope setter ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_context_set_global_scope`; callers must pass valid
/// context and scope handles owned by generated code.
unsafe fn eval_context_set_global_scope_inner(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    if !context.set_global_scope(scope) {
        return EvalStatus::RuntimeFatal.code();
    }
    EvalStatus::Ok.code()
}

/// Runs the class-scope push ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_context_push_class_scope`; callers must pass a valid
/// context and readable UTF-8 class-name byte slices.
unsafe fn eval_context_push_class_scope_inner(
    ctx: *mut ElephcEvalContext,
    class_ptr: *const u8,
    class_len: u64,
    called_class_ptr: *const u8,
    called_class_len: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    let Ok(class_name) = abi_name_to_string(class_ptr, class_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(called_class_name) = abi_name_to_string(called_class_ptr, called_class_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let class_name = class_name.trim_start_matches('\\').to_string();
    if class_name.is_empty() {
        return EvalStatus::RuntimeFatal.code();
    }
    let called_class_name = called_class_name.trim_start_matches('\\');
    let called_class_name = if called_class_name.is_empty() {
        class_name.clone()
    } else {
        called_class_name.to_string()
    };
    let called_class_name = context
        .native_frame_called_class_override(&class_name, &called_class_name)
        .unwrap_or(called_class_name);
    context.push_class_scope(class_name);
    context.push_called_class_scope(called_class_name);
    EvalStatus::Ok.code()
}

/// Runs the class-scope pop ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_context_pop_class_scope`; callers must pass a valid
/// context handle created by the eval bridge.
unsafe fn eval_context_pop_class_scope_inner(ctx: *mut ElephcEvalContext) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    context.pop_called_class_scope();
    context.pop_class_scope();
    EvalStatus::Ok.code()
}

/// Runs the native-frame called-class lookup after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_native_frame_called_class_override`; generated code
/// passes writable stack slots for both out-parameters.
unsafe fn eval_native_frame_called_class_override_inner(
    class_ptr: *const u8,
    class_len: u64,
    out_ptr: *mut *const u8,
    out_len: *mut u64,
) -> i32 {
    if out_ptr.is_null() || out_len.is_null() {
        return 0;
    }
    unsafe {
        *out_ptr = ptr::null();
        *out_len = 0;
    }
    let Ok(class_name) = abi_name_to_string(class_ptr, class_len) else {
        return 0;
    };
    let Some((called_ptr, called_len)) = native_frame_called_class_override_bytes(&class_name)
    else {
        return 0;
    };
    let Ok(called_len) = u64::try_from(called_len) else {
        return 0;
    };
    unsafe {
        *out_ptr = called_ptr;
        *out_len = called_len;
    }
    1
}
