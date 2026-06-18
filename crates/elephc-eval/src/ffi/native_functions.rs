//! Purpose:
//! Exports registration of generated native PHP callbacks into an eval context.
//! Eval fragments use this metadata to call AOT functions through descriptor
//! invokers while preserving PHP-visible parameter names.
//!
//! Called from:
//! - Generated EIR backend assembly before fragments can call AOT functions.
//!
//! Key details:
//! - Invalid names, handles, descriptors, or indexes fail closed as `false`.
//! - Function names are stored under their PHP case-insensitive folded key.

use super::util::abi_name_to_string;
use crate::abi::{ElephcEvalContext, ABI_VERSION};
use crate::context::{NativeFunction, NativeFunctionInvoker};
use std::ffi::c_void;

/// Registers a generated native PHP function callback in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`. `descriptor` and `invoker` must follow
/// the descriptor-invoker ABI emitted by generated code.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_function(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    descriptor: *mut c_void,
    invoker: Option<NativeFunctionInvoker>,
    param_count: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_function_inner(ctx, name_ptr, name_len, descriptor, invoker, param_count)
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP function parameter name in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Function and parameter name
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_function_param(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    param_index: u64,
    param_name_ptr: *const u8,
    param_name_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_function_param_inner(
            ctx,
            function_name_ptr,
            function_name_len,
            param_index,
            param_name_ptr,
            param_name_len,
        )
    })
    .unwrap_or(0)
}

/// Runs the native registration ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_function`; invalid handles, names, or
/// callback pointers fail closed as `false`.
unsafe fn register_native_function_inner(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    descriptor: *mut c_void,
    invoker: Option<NativeFunctionInvoker>,
    param_count: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION || descriptor.is_null() {
        return 0;
    }
    let Some(invoker) = invoker else {
        return 0;
    };
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return 0;
    };
    let Ok(param_count) = usize::try_from(param_count) else {
        return 0;
    };
    let function = NativeFunction::new(descriptor, invoker, param_count);
    i32::from(
        context
            .define_native_function(name.to_ascii_lowercase(), function)
            .is_ok(),
    )
}

/// Runs the native parameter-name registration ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_function_param`; invalid handles,
/// names, or indexes fail closed as `false`.
unsafe fn register_native_function_param_inner(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    param_index: u64,
    param_name_ptr: *const u8,
    param_name_len: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(function_name) = abi_name_to_string(function_name_ptr, function_name_len) else {
        return 0;
    };
    let Ok(param_name) = abi_name_to_string(param_name_ptr, param_name_len) else {
        return 0;
    };
    let Ok(param_index) = usize::try_from(param_index) else {
        return 0;
    };
    i32::from(context.define_native_function_param(
        &function_name.to_ascii_lowercase(),
        param_index,
        param_name,
    ))
}
