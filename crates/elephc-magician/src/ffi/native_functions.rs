//! Purpose:
//! Exports registration of generated native PHP callbacks into an eval context.
//! Eval fragments use this metadata to call AOT functions through descriptor
//! invokers while preserving PHP-visible parameter names and defaults.
//!
//! Called from:
//! - Generated EIR backend assembly before fragments can call AOT functions.
//!
//! Key details:
//! - Invalid names, handles, descriptors, or indexes fail closed as `false`.
//! - Function names are stored under their PHP case-insensitive folded key.

use super::native_methods::{
    native_callable_array_default, native_callable_object_default, native_callable_scalar_default,
    native_callable_type_from_abi, NativeCallableTypePosition,
};
use super::util::abi_name_to_string;
use crate::abi::{ElephcEvalContext, ABI_VERSION};
use crate::context::{NativeCallableDefault, NativeFunction, NativeFunctionInvoker};
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

/// Runs native function bridge-support registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_function_bridge_support`; invalid
/// handles or names fail closed as `false`.
unsafe fn register_native_function_bridge_support_inner(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    supported: i32,
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
    i32::from(context.define_native_function_bridge_supported(
        &function_name.to_ascii_lowercase(),
        supported != 0,
    ))
}

/// Runs native function parameter-flags registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_function_param_flags`; invalid
/// handles, names, or indexes fail closed as `false`.
unsafe fn register_native_function_param_flags_inner(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    param_index: u64,
    is_by_ref: i32,
    is_variadic: i32,
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
    let Ok(param_index) = usize::try_from(param_index) else {
        return 0;
    };
    let function_name = function_name.to_ascii_lowercase();
    if !context.define_native_function_param_by_ref(&function_name, param_index, is_by_ref != 0) {
        return 0;
    }
    if is_variadic != 0 {
        return i32::from(context.define_native_function_variadic_param(
            &function_name,
            param_index,
        ));
    }
    1
}

/// Runs native function parameter-type registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_function_param_type`; invalid handles,
/// names, indexes, or type specs fail closed as `false`.
unsafe fn register_native_function_param_type_inner(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    param_index: u64,
    type_spec_ptr: *const u8,
    type_spec_len: u64,
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
    let Ok(param_index) = usize::try_from(param_index) else {
        return 0;
    };
    let Some(param_type) = native_callable_type_from_abi(
        type_spec_ptr,
        type_spec_len,
        NativeCallableTypePosition::Parameter,
    ) else {
        return 0;
    };
    i32::from(context.define_native_function_param_type(
        &function_name.to_ascii_lowercase(),
        param_index,
        param_type,
    ))
}

/// Runs native function return-type registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_function_return_type`; invalid handles,
/// names, or type specs fail closed as `false`.
unsafe fn register_native_function_return_type_inner(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    type_spec_ptr: *const u8,
    type_spec_len: u64,
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
    let Some(return_type) = native_callable_type_from_abi(
        type_spec_ptr,
        type_spec_len,
        NativeCallableTypePosition::Return,
    ) else {
        return 0;
    };
    i32::from(context.define_native_function_return_type(
        &function_name.to_ascii_lowercase(),
        return_type,
    ))
}

/// Runs native function scalar-default registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_function_param_default_scalar`; invalid
/// handles, names, indexes, or default kinds fail closed as `false`.
unsafe fn register_native_function_param_default_scalar_inner(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    param_index: u64,
    default_kind: u64,
    default_payload: u64,
) -> i32 {
    let Some(default) = native_callable_scalar_default(default_kind, default_payload) else {
        return 0;
    };
    register_native_function_param_default_inner(
        ctx,
        function_name_ptr,
        function_name_len,
        param_index,
        default,
    )
}

/// Runs native function string-default registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_function_param_default_string`; invalid
/// handles, names, or indexes fail closed as `false`.
unsafe fn register_native_function_param_default_string_inner(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    param_index: u64,
    default_ptr: *const u8,
    default_len: u64,
) -> i32 {
    let Ok(default) = abi_name_to_string(default_ptr, default_len) else {
        return 0;
    };
    register_native_function_param_default_inner(
        ctx,
        function_name_ptr,
        function_name_len,
        param_index,
        NativeCallableDefault::String(default),
    )
}

/// Runs native function object-default registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_function_param_default_object`; invalid
/// handles, names, indexes, or object specs fail closed as `false`.
unsafe fn register_native_function_param_default_object_inner(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    param_index: u64,
    spec_ptr: *const u8,
    spec_len: u64,
) -> i32 {
    let Some(default) = native_callable_object_default(spec_ptr, spec_len) else {
        return 0;
    };
    register_native_function_param_default_inner(
        ctx,
        function_name_ptr,
        function_name_len,
        param_index,
        default,
    )
}

/// Runs native function array-default registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_function_param_default_array`; invalid
/// handles, names, indexes, or array specs fail closed as `false`.
unsafe fn register_native_function_param_default_array_inner(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    param_index: u64,
    spec_ptr: *const u8,
    spec_len: u64,
) -> i32 {
    let Some(default) = native_callable_array_default(spec_ptr, spec_len) else {
        return 0;
    };
    register_native_function_param_default_inner(
        ctx,
        function_name_ptr,
        function_name_len,
        param_index,
        default,
    )
}

/// Records a native function parameter default by folded function name.
///
/// # Safety
/// `ctx` and `function_name_ptr` must be valid for their declared use; callers
/// are the exported ABI wrappers above.
unsafe fn register_native_function_param_default_inner(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    param_index: u64,
    default: NativeCallableDefault,
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
    let Ok(param_index) = usize::try_from(param_index) else {
        return 0;
    };
    i32::from(context.define_native_function_param_default(
        &function_name.to_ascii_lowercase(),
        param_index,
        default,
    ))
}
