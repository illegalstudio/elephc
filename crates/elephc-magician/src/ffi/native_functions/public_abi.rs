//! Purpose:
//! Exposes C ABI entry points for generated native PHP function signatures,
//! types, flags, bridge support, and defaults.
//!
//! Called from:
//! - Generated EIR backend assembly during native function registration.
//!
//! Key details:
//! - Every entry point installs a panic boundary before internal validation.

use super::*;

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

/// Registers whether a generated native PHP function can be invoked through its bridge.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Function name must be readable for
/// its declared byte length.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_function_bridge_support(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    supported: i32,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_function_bridge_support_inner(
            ctx,
            function_name_ptr,
            function_name_len,
            supported,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP function parameter's by-ref and variadic flags.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Function name must be readable for
/// its declared byte length.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_function_param_flags(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    param_index: u64,
    is_by_ref: i32,
    is_variadic: i32,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_function_param_flags_inner(
            ctx,
            function_name_ptr,
            function_name_len,
            param_index,
            is_by_ref,
            is_variadic,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP function parameter type in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Function name and type-spec
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_function_param_type(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    param_index: u64,
    type_spec_ptr: *const u8,
    type_spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_function_param_type_inner(
            ctx,
            function_name_ptr,
            function_name_len,
            param_index,
            type_spec_ptr,
            type_spec_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP function return type in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Function name and type-spec
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_function_return_type(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    type_spec_ptr: *const u8,
    type_spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_function_return_type_inner(
            ctx,
            function_name_ptr,
            function_name_len,
            type_spec_ptr,
            type_spec_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP function scalar parameter default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Function name must be readable for
/// its declared byte length.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_function_param_default_scalar(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    param_index: u64,
    default_kind: u64,
    default_payload: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_function_param_default_scalar_inner(
            ctx,
            function_name_ptr,
            function_name_len,
            param_index,
            default_kind,
            default_payload,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP function string parameter default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Function name and default string
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_function_param_default_string(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    param_index: u64,
    default_ptr: *const u8,
    default_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_function_param_default_string_inner(
            ctx,
            function_name_ptr,
            function_name_len,
            param_index,
            default_ptr,
            default_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP function object parameter default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Function name and encoded default
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_function_param_default_object(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    param_index: u64,
    spec_ptr: *const u8,
    spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_function_param_default_object_inner(
            ctx,
            function_name_ptr,
            function_name_len,
            param_index,
            spec_ptr,
            spec_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP function array parameter default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Function name and encoded default
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_function_param_default_array(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    param_index: u64,
    spec_ptr: *const u8,
    spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_function_param_default_array_inner(
            ctx,
            function_name_ptr,
            function_name_len,
            param_index,
            spec_ptr,
            spec_len,
        )
    })
    .unwrap_or(0)
}
