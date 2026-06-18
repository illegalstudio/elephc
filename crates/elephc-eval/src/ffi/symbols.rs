//! Purpose:
//! Exports dynamic symbol probes and constant fetching for eval-created state.
//! Generated code calls these after eval barriers to observe dynamic functions,
//! constants, and classes registered by interpreted fragments.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_*exists` symbols.
//!
//! Key details:
//! - Existence probes fail closed as `false` on invalid ABI inputs.
//! - Constant fetch retains the boxed cell before handing it back to generated code.

use super::util::abi_name_to_string;
#[cfg(not(test))]
use super::util::clear_result;
use crate::abi::{ElephcEvalContext, ABI_VERSION};
#[cfg(not(test))]
use crate::abi::ElephcEvalResult;
#[cfg(not(test))]
use crate::errors::EvalStatus;
#[cfg(not(test))]
use crate::interpreter::RuntimeValueOps;
#[cfg(not(test))]
use crate::runtime_hooks::ElephcRuntimeOps;

/// Checks whether a function was previously declared through `eval()`.
///
/// # Safety
/// `ctx` must be null or a valid eval context handle. `name_ptr` must be
/// readable for `name_len` bytes when `name_len > 0`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_function_exists(
    ctx: *const ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe { eval_function_exists_inner(ctx, name_ptr, name_len) })
        .unwrap_or(0)
}

/// Checks whether a constant was previously defined through `eval()`.
///
/// # Safety
/// `ctx` must be null or a valid eval context handle. `name_ptr` must be
/// readable for `name_len` bytes when `name_len > 0`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_constant_exists(
    ctx: *const ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe { eval_constant_exists_inner(ctx, name_ptr, name_len) })
        .unwrap_or(0)
}

/// Checks whether a class was previously declared through `eval()`.
///
/// # Safety
/// `ctx` must be null or a valid eval context handle. `name_ptr` must be
/// readable for `name_len` bytes when `name_len > 0`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_dynamic_class_exists(
    ctx: *const ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_dynamic_class_exists_inner(ctx, name_ptr, name_len)
    })
    .unwrap_or(0)
}

/// Fetches a constant previously defined through `eval()`.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_constant_fetch(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_constant_fetch_inner(ctx, name_ptr, name_len, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Runs the eval function-exists ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_function_exists`; invalid handles or unreadable name
/// storage fail closed as `false`.
unsafe fn eval_function_exists_inner(
    ctx: *const ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
) -> i32 {
    let Some(context) = ctx.as_ref() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return 0;
    };
    i32::from(context.has_function(&name.to_ascii_lowercase()))
}

/// Runs the eval constant-exists ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_constant_exists`; invalid handles or unreadable name
/// storage fail closed as `false`.
unsafe fn eval_constant_exists_inner(
    ctx: *const ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
) -> i32 {
    let Some(context) = ctx.as_ref() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return 0;
    };
    i32::from(context.has_constant(&name))
}

/// Runs the eval dynamic-class-exists ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_dynamic_class_exists`; invalid handles or unreadable
/// name storage fail closed as `false`.
unsafe fn eval_dynamic_class_exists_inner(
    ctx: *const ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
) -> i32 {
    let Some(context) = ctx.as_ref() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return 0;
    };
    i32::from(context.has_class(&name))
}

/// Runs the eval constant-fetch ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_constant_fetch`; callers must provide a valid context,
/// readable constant-name bytes, and optional writable result storage.
#[cfg(not(test))]
unsafe fn eval_constant_fetch_inner(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
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
    clear_result(out);
    let Some(value) = context.constant(&name) else {
        return EvalStatus::RuntimeFatal.code();
    };
    if out.is_null() {
        return EvalStatus::Ok.code();
    }
    let mut values = ElephcRuntimeOps::new();
    match values.retain(value) {
        Ok(result) => {
            (*out).kind = 0;
            (*out).value_cell = result.as_ptr();
            (*out).error = std::ptr::null_mut();
            EvalStatus::Ok.code()
        }
        Err(status) => status.code(),
    }
}
