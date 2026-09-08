//! Purpose:
//! Exports post-barrier callable dispatch and probes for callback values that
//! may reference eval-declared functions, methods, or objects. Generated code
//! uses this ABI when native descriptor metadata cannot answer dynamically.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_callable_call_array`.
//! - Generated EIR backend assembly through `__elephc_eval_is_callable`.
//!
//! Key details:
//! - Callback and argument containers are boxed Mixed cells owned by generated
//!   code. Dispatch results and uncaught throwables are returned through
//!   `ElephcEvalResult`; probe failures fail closed as `false`.

use super::util::{clear_result, write_outcome};
use crate::abi::{ElephcEvalContext, ElephcEvalResult, ABI_VERSION};
use crate::context::native_frame_called_class_override_context;
use crate::errors::EvalStatus;
use crate::interpreter::{self, RuntimeValueOps};
use crate::runtime_hooks::ElephcRuntimeOps;
use crate::value::{RuntimeCell, RuntimeCellHandle};

/// Checks whether a callback value is callable in the eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle and `callback` must point at a
/// boxed runtime cell.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_is_callable(
    ctx: *mut ElephcEvalContext,
    callback: *mut RuntimeCell,
) -> i32 {
    std::panic::catch_unwind(|| unsafe { eval_is_callable_inner(ctx, callback) }).unwrap_or(0)
}

/// Dispatches a callback value with a PHP argument array through the eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `callback` and `arg_array` must
/// point at boxed runtime cells, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_callable_call_array(
    ctx: *mut ElephcEvalContext,
    callback: *mut RuntimeCell,
    arg_array: *mut RuntimeCell,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_callable_call_array_inner(ctx, callback, arg_array, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Materializes the effective late-static method callable for an active AOT frame.
///
/// # Safety
/// `frame_class_ptr` and `method_ptr` must describe readable UTF-8 byte ranges,
/// and both output pointers must be null or writable pointer slots. Status `-1`
/// is the only benign miss; non-negative values use `EvalStatus` codes.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_native_frame_static_method_callable(
    frame_class_ptr: *const u8,
    frame_class_len: u64,
    method_ptr: *const u8,
    method_len: u64,
    out_callback: *mut *mut RuntimeCell,
    out_context: *mut *mut ElephcEvalContext,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_native_frame_static_method_callable_inner(
            frame_class_ptr,
            frame_class_len,
            method_ptr,
            method_len,
            out_callback,
            out_context,
        )
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Associates a native descriptor identity with its captured eval Closure target.
///
/// # Safety
/// `ctx` must be a valid eval context, `callback` a live eval Closure cell, and
/// `alias_identity` the stable native object payload address exposed to Magician.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_alias_callable_identity(
    ctx: *mut ElephcEvalContext,
    callback: *mut RuntimeCell,
    alias_identity: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_alias_callable_identity_inner(ctx, callback, alias_identity)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Tears down the Magician-owned captures of one eval callback descriptor.
///
/// # Safety
/// `ctx` and `callback` must be the live owners captured by the descriptor, and
/// `alias_identity` must be the identity registered for that descriptor.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_release_callable_descriptor(
    ctx: *mut ElephcEvalContext,
    callback: *mut RuntimeCell,
    alias_identity: u64,
) {
    let mut final_source_identity = None;
    run_callable_cleanup_stage(|| {
        if let Some(context) = unsafe { ctx.as_mut() } {
            context.unregister_closure_object_target(alias_identity);
        }
    });
    let released_callback = run_callable_cleanup_stage_result(|| {
        let Some(context) = (unsafe { ctx.as_mut() }) else {
            return false;
        };
        let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
        let callback = RuntimeCellHandle::from_raw(callback);
        final_source_identity = values
            .final_object_identity_for_release(callback)
            .ok()
            .flatten();
        values.release(callback).is_ok()
    })
    .unwrap_or(false);
    if released_callback {
        run_callable_cleanup_stage(|| {
            if let (Some(context), Some(identity)) =
                (unsafe { ctx.as_mut() }, final_source_identity)
            {
                context.unregister_closure_object_target(identity);
            }
        });
    }
    unsafe { crate::ffi::context::__elephc_eval_context_free(ctx) };
}

/// Runs one descriptor teardown stage while containing its panic payload.
#[cfg(not(test))]
fn run_callable_cleanup_stage(stage: impl FnOnce()) {
    if let Err(payload) =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(stage))
    {
        std::mem::forget(payload);
    }
}

/// Runs one descriptor teardown stage that reports a value when it completes.
#[cfg(not(test))]
fn run_callable_cleanup_stage_result<T>(stage: impl FnOnce() -> T) -> Option<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(stage)) {
        Ok(value) => Some(value),
        Err(payload) => {
            std::mem::forget(payload);
            None
        }
    }
}

/// Runs the eval callable-probe ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_is_callable`; invalid handles fail closed as false.
#[cfg(not(test))]
unsafe fn eval_is_callable_inner(
    ctx: *mut ElephcEvalContext,
    callback: *mut RuntimeCell,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION || callback.is_null() {
        return 0;
    }
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_is_callable(
        context,
        RuntimeCellHandle::from_raw(callback),
        &mut values,
    ) {
        Ok(callable) => i32::from(callable),
        Err(_) => 0,
    }
}

/// Resolves and builds one late-static eval callable without crossing a panic boundary.
#[cfg(not(test))]
unsafe fn eval_native_frame_static_method_callable_inner(
    frame_class_ptr: *const u8,
    frame_class_len: u64,
    method_ptr: *const u8,
    method_len: u64,
    out_callback: *mut *mut RuntimeCell,
    out_context: *mut *mut ElephcEvalContext,
) -> i32 {
    if !out_callback.is_null() {
        *out_callback = std::ptr::null_mut();
    }
    if !out_context.is_null() {
        *out_context = std::ptr::null_mut();
    }
    if out_callback.is_null() || out_context.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let Ok(frame_class) = super::util::abi_name_to_string(frame_class_ptr, frame_class_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(method) = super::util::abi_name_to_string(method_ptr, method_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Some((context, called_class)) =
        native_frame_called_class_override_context(&frame_class)
    else {
        return -1;
    };
    let Some(context) = context.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let callable = match interpreter::execute_context_static_method_callable(
        context,
        &called_class,
        &method,
        &mut values,
    ) {
        Ok(callable) => callable,
        Err(status) => {
            if status == EvalStatus::UncaughtThrowable {
                if let Some(error) = context.take_pending_throw() {
                    *out_callback = error.as_ptr();
                }
            }
            return status.code();
        }
    };
    context.acquire_owner();
    *out_callback = callable.as_ptr();
    *out_context = context;
    EvalStatus::Ok.code()
}

/// Registers one native descriptor as an alias of an existing eval Closure identity.
#[cfg(not(test))]
unsafe fn eval_alias_callable_identity_inner(
    ctx: *mut ElephcEvalContext,
    callback: *mut RuntimeCell,
    alias_identity: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION || callback.is_null() || alias_identity == 0 {
        return EvalStatus::RuntimeFatal.code();
    }
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let Ok(source_identity) = values.object_identity(RuntimeCellHandle::from_raw(callback)) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Some(target) = context.closure_object_target(source_identity).cloned() else {
        return EvalStatus::RuntimeFatal.code();
    };
    context.register_closure_object_target(alias_identity, target);
    EvalStatus::Ok.code()
}

/// Runs the eval callable-array ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_callable_call_array`; callers must provide a valid
/// context and boxed callback/argument-array cells.
#[cfg(not(test))]
unsafe fn eval_callable_call_array_inner(
    ctx: *mut ElephcEvalContext,
    callback: *mut RuntimeCell,
    arg_array: *mut RuntimeCell,
    out: *mut ElephcEvalResult,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    if callback.is_null() || arg_array.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_callable_call_array_outcome(
        context,
        RuntimeCellHandle::from_raw(callback),
        RuntimeCellHandle::from_raw(arg_array),
        &mut values,
    ) {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => status.code(),
    }
}
