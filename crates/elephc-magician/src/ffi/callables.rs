//! Purpose:
//! Exports post-barrier callable dispatch for callback values that may reference
//! eval-declared functions, methods, or objects. Generated code uses this ABI
//! after native descriptor dispatch cannot resolve a dynamic callable array.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_callable_call_array`.
//!
//! Key details:
//! - Callback and argument containers are boxed Mixed cells owned by generated
//!   code. Results and uncaught throwables are returned through `ElephcEvalResult`.

use super::util::{clear_result, write_outcome};
use crate::abi::{ElephcEvalContext, ElephcEvalResult, ABI_VERSION};
use crate::errors::EvalStatus;
use crate::interpreter;
use crate::runtime_hooks::ElephcRuntimeOps;
use crate::value::{RuntimeCell, RuntimeCellHandle};

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
