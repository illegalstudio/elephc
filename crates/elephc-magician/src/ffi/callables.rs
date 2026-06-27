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
