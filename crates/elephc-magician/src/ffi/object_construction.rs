//! Purpose:
//! Exports post-barrier construction and method calls for classes declared by
//! eval fragments. Generated code uses this ABI when native object operations
//! may target a dynamic eval class instead of an AOT class.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_new_object`.
//! - Generated EIR backend assembly through `__elephc_eval_try_new_object`.
//! - Generated EIR backend assembly through `__elephc_eval_method_call`.
//! - Generated EIR backend assembly through `__elephc_eval_static_method_call`.
//!
//! Key details:
//! - The ABI currently accepts positional arguments already boxed as Mixed cells.
//! - Calls return either a value cell or an uncaught throwable cell through
//!   `ElephcEvalResult`.

use super::util::{abi_name_to_string, clear_result, write_outcome};
use crate::abi::{ElephcEvalContext, ElephcEvalResult, ABI_VERSION};
use crate::context::native_frame_called_class_override_context;
use crate::errors::EvalStatus;
use crate::interpreter;
use crate::runtime_hooks::ElephcRuntimeOps;
use crate::value::{RuntimeCell, RuntimeCellHandle};
use std::slice;

/// Constructs a class previously declared through `eval()` with positional cells.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`. `args` must be readable for
/// `arg_count` runtime-cell pointers when `arg_count > 0`, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_new_object(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    args: *const *mut RuntimeCell,
    arg_count: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_new_object_inner(ctx, name_ptr, name_len, args, arg_count, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Attempts to construct an eval-declared class with positional cells.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`. `args` must be readable for
/// `arg_count` runtime-cell pointers when `arg_count > 0`, and `out` may be null.
/// Returns -1 when the class name is not declared in the eval context.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_try_new_object(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    args: *const *mut RuntimeCell,
    arg_count: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_try_new_object_inner(ctx, name_ptr, name_len, args, arg_count, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Calls a method on a value that may be an eval-created object.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `object` must point at a boxed
/// runtime cell. `method_ptr` must be readable for `method_len` bytes when
/// `method_len > 0`. `arg_pack` points at a native-word count followed by that
/// many runtime-cell pointers, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_method_call(
    ctx: *mut ElephcEvalContext,
    object: *mut RuntimeCell,
    method_ptr: *const u8,
    method_len: u64,
    arg_pack: *const usize,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_method_call_inner(ctx, object, method_ptr, method_len, arg_pack, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Calls a static method on a class previously declared through `eval()`.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `target_ptr` must be readable
/// for `target_len` bytes and contain `ClassName::method`. `arg_pack` points at
/// a native-word count followed by that many runtime-cell pointers, and `out`
/// may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_static_method_call(
    ctx: *mut ElephcEvalContext,
    target_ptr: *const u8,
    target_len: u64,
    arg_pack: *const usize,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_static_method_call_inner(ctx, target_ptr, target_len, arg_pack, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Calls a static method through the current eval late-static AOT-frame override.
///
/// # Safety
/// `frame_class_ptr` and `method_ptr` must be readable UTF-8 byte ranges.
/// `arg_pack` points at a native-word count followed by that many runtime-cell
/// pointers, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_native_frame_static_method_call(
    frame_class_ptr: *const u8,
    frame_class_len: u64,
    method_ptr: *const u8,
    method_len: u64,
    arg_pack: *const usize,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_native_frame_static_method_call_inner(
            frame_class_ptr,
            frame_class_len,
            method_ptr,
            method_len,
            arg_pack,
            out,
        )
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Runs the dynamic object-construction ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_new_object`; callers must provide a valid context,
/// readable class-name bytes, and readable argument pointer storage.
#[cfg(not(test))]
unsafe fn eval_new_object_inner(
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
    match interpreter::execute_context_new_object_outcome(context, &name, args, &mut values) {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => status.code(),
    }
}

/// Runs the dynamic object-construction probe ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_try_new_object`; callers must provide a valid context,
/// readable class-name bytes, and readable argument pointer storage.
#[cfg(not(test))]
unsafe fn eval_try_new_object_inner(
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
    match interpreter::execute_context_try_new_object_outcome(context, &name, args, &mut values) {
        Ok(Some(outcome)) => write_outcome(outcome, out).code(),
        Ok(None) => -1,
        Err(status) => status.code(),
    }
}

/// Runs the dynamic static-method call ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_static_method_call`; callers must provide a valid
/// context, readable `ClassName::method` bytes, and a readable argument pack.
#[cfg(not(test))]
unsafe fn eval_static_method_call_inner(
    ctx: *mut ElephcEvalContext,
    target_ptr: *const u8,
    target_len: u64,
    arg_pack: *const usize,
    out: *mut ElephcEvalResult,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    if arg_pack.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let Ok(target) = abi_name_to_string(target_ptr, target_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Some((class_name, method)) = target.rsplit_once("::") else {
        return EvalStatus::RuntimeFatal.code();
    };
    let arg_count = *arg_pack;
    let arg_ptrs = arg_pack.add(1) as *const *mut RuntimeCell;
    let args = if arg_count == 0 {
        Vec::new()
    } else {
        slice::from_raw_parts(arg_ptrs, arg_count)
            .iter()
            .map(|arg| RuntimeCellHandle::from_raw(*arg))
            .collect()
    };
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_static_method_call_outcome(
        context, class_name, method, args, &mut values,
    ) {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => status.code(),
    }
}

/// Runs a late-static eval override call from a generated/AOT frame.
///
/// # Safety
/// Mirrors `__elephc_eval_native_frame_static_method_call`; callers must
/// provide readable frame/method names and a readable argument pack.
#[cfg(not(test))]
unsafe fn eval_native_frame_static_method_call_inner(
    frame_class_ptr: *const u8,
    frame_class_len: u64,
    method_ptr: *const u8,
    method_len: u64,
    arg_pack: *const usize,
    out: *mut ElephcEvalResult,
) -> i32 {
    if arg_pack.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let Ok(frame_class) = abi_name_to_string(frame_class_ptr, frame_class_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(method) = abi_name_to_string(method_ptr, method_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Some((context, called_class)) = native_frame_called_class_override_context(&frame_class)
    else {
        return -1;
    };
    let Some(context) = context.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    let arg_count = *arg_pack;
    let arg_ptrs = arg_pack.add(1) as *const *mut RuntimeCell;
    let args = if arg_count == 0 {
        Vec::new()
    } else {
        slice::from_raw_parts(arg_ptrs, arg_count)
            .iter()
            .map(|arg| RuntimeCellHandle::from_raw(*arg))
            .collect()
    };
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_static_method_call_outcome(
        context,
        &called_class,
        &method,
        args,
        &mut values,
    ) {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => status.code(),
    }
}

/// Runs the dynamic method-call ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_method_call`; callers must provide a valid context,
/// boxed object cell, readable method-name bytes, and a readable argument pack.
#[cfg(not(test))]
unsafe fn eval_method_call_inner(
    ctx: *mut ElephcEvalContext,
    object: *mut RuntimeCell,
    method_ptr: *const u8,
    method_len: u64,
    arg_pack: *const usize,
    out: *mut ElephcEvalResult,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    if object.is_null() || arg_pack.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let Ok(method) = abi_name_to_string(method_ptr, method_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let arg_count = *arg_pack;
    let arg_ptrs = arg_pack.add(1) as *const *mut RuntimeCell;
    let args = if arg_count == 0 {
        Vec::new()
    } else {
        slice::from_raw_parts(arg_ptrs, arg_count)
            .iter()
            .map(|arg| RuntimeCellHandle::from_raw(*arg))
            .collect()
    };
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_method_call_outcome(
        context,
        RuntimeCellHandle::from_raw(object),
        &method,
        args,
        &mut values,
    ) {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => status.code(),
    }
}
