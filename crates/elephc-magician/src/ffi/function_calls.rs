//! Purpose:
//! Exports post-barrier calls into functions declared by eval fragments.
//! Generated code can call zero-argument, positional, or argument-array forms
//! after probing dynamic function existence.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_call_function*`.
//!
//! Key details:
//! - Calls return either a value cell or an uncaught throwable cell through
//!   `ElephcEvalResult`.
//! - Argument pointer arrays are borrowed from generated code and not released here.

use super::util::{abi_name_to_string, clear_result, write_outcome};
use crate::abi::{ElephcEvalContext, ElephcEvalResult, ABI_VERSION};
use crate::errors::EvalStatus;
use crate::interpreter;
use crate::runtime_hooks::ElephcRuntimeOps;
use crate::value::{RuntimeCell, RuntimeCellHandle};
use std::slice;

/// Calls a zero-argument function previously declared through `eval()`.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_call_function_zero_args(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        call_eval_function_inner(ctx, name_ptr, name_len, std::ptr::null(), 0, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Calls a function previously declared through `eval()` with positional cells.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`. `args` must be readable for
/// `arg_count` runtime-cell pointers when `arg_count > 0`, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_call_function(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    args: *const *mut RuntimeCell,
    arg_count: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        call_eval_function_inner(ctx, name_ptr, name_len, args, arg_count, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Calls a function previously declared through `eval()` with an argument array/hash.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`. `arg_array` must be a boxed Mixed
/// indexed or associative array cell, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_call_function_array(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    arg_array: *mut RuntimeCell,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        call_eval_function_array_inner(ctx, name_ptr, name_len, arg_array, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Runs the dynamic function-call ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_call_function`; callers must provide a valid context,
/// readable function-name bytes, and readable argument pointer storage.
#[cfg(not(test))]
unsafe fn call_eval_function_inner(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    args: *const *mut RuntimeCell,
    arg_count: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(arg_count) = usize::try_from(arg_count) else {
        return EvalStatus::RuntimeFatal.code();
    };
    if arg_count > 0 && args.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let args = if arg_count == 0 {
        Vec::new()
    } else {
        slice::from_raw_parts(args, arg_count)
            .iter()
            .map(|arg| RuntimeCellHandle::from_raw(*arg))
            .collect()
    };
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_function_outcome(
        context,
        &name.to_ascii_lowercase(),
        args,
        &mut values,
    ) {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => status.code(),
    }
}

/// Runs the dynamic function-call-array ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_call_function_array`; callers must provide a valid
/// context, readable function-name bytes, and a boxed array/hash argument cell.
#[cfg(not(test))]
unsafe fn call_eval_function_array_inner(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    arg_array: *mut RuntimeCell,
    out: *mut ElephcEvalResult,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    if arg_array.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_function_call_array_outcome(
        context,
        &name.to_ascii_lowercase(),
        RuntimeCellHandle::from_raw(arg_array),
        &mut values,
    ) {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => status.code(),
    }
}
