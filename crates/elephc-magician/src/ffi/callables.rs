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
use crate::errors::EvalStatus;
use crate::interpreter;
use crate::interpreter::RuntimeValueOps;
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

/// Rebinds `$this` on an eval `Closure` object for the generated `Closure::bind` runtime.
///
/// Returns one with an owned boxed `Closure` in `out`, or zero when the value is not an eval
/// closure or the binding failed. Installed into the generated runtime through
/// `__elephc_eval_install_closure_bind_hook_v1`, so a callback-adapter descriptor reaching
/// `__rt_closure_bind` can be rebound instead of aborting as an unsupported capture shape.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `closure` must point at the boxed callback cell
/// the adapter descriptor captured; it stays borrowed. `new_this` must be a live raw elephc
/// object pointer. `out` must point at a writable cell-pointer slot.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_closure_bind_this(
    ctx: *mut ElephcEvalContext,
    closure: *mut RuntimeCell,
    new_this: *mut RuntimeCell,
    out: *mut *mut RuntimeCell,
) -> u64 {
    std::panic::catch_unwind(|| unsafe { eval_closure_bind_this_inner(ctx, closure, new_this, out) })
        .unwrap_or(0)
}

/// Runs the closure rebinding ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_closure_bind_this`.
#[cfg(not(test))]
unsafe fn eval_closure_bind_this_inner(
    ctx: *mut ElephcEvalContext,
    closure: *mut RuntimeCell,
    new_this: *mut RuntimeCell,
    out: *mut *mut RuntimeCell,
) -> u64 {
    if out.is_null() {
        return 0;
    }
    unsafe { *out = std::ptr::null_mut(); }
    let Some(context) = (unsafe { ctx.as_mut() }) else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION || closure.is_null() || new_this.is_null() {
        return 0;
    }
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let Ok(receiver) = ElephcRuntimeOps::object_from_raw(new_this) else {
        return 0;
    };
    let bound = interpreter::execute_context_closure_bind_this(
        context,
        RuntimeCellHandle::from_raw(closure).borrowed(),
        receiver,
        &mut values,
    );
    // The bound closure retains the receiver as its own child, so this bridge's boxed owner
    // must not outlive the call; keeping it pinned the receiver and the closure's metadata.
    let released = values.release(receiver);
    match (bound, released) {
        (Ok(bound), Ok(())) => {
            unsafe { *out = bound.as_ptr(); }
            1
        }
        _ => 0,
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
        RuntimeCellHandle::from_raw(callback).borrowed(),
        &mut values,
    ) {
        Ok(callable) => i32::from(callable),
        Err(_) => 0,
    }
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
        RuntimeCellHandle::from_raw(callback).borrowed(),
        RuntimeCellHandle::from_raw(arg_array).borrowed(),
        &mut values,
    ) {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => status.code(),
    }
}
