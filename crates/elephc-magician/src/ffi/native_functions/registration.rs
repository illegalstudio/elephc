//! Purpose:
//! Validates and records generated native function callbacks and callable
//! metadata after the public ABI panic boundary.
//!
//! Called from:
//! - `super::public_abi` registration entry points.
//!
//! Key details:
//! - Invalid handles, names, descriptors, types, defaults, or indexes fail closed.

use super::*;

/// Runs the native registration ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_function`; invalid handles, names, or
/// callback pointers fail closed as `false`.
pub(super) unsafe fn register_native_function_inner(
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
pub(super) unsafe fn register_native_function_param_inner(
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
pub(super) unsafe fn register_native_function_bridge_support_inner(
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
pub(super) unsafe fn register_native_function_param_flags_inner(
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
pub(super) unsafe fn register_native_function_param_type_inner(
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
pub(super) unsafe fn register_native_function_return_type_inner(
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
pub(super) unsafe fn register_native_function_param_default_scalar_inner(
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
pub(super) unsafe fn register_native_function_param_default_string_inner(
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
pub(super) unsafe fn register_native_function_param_default_object_inner(
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
pub(super) unsafe fn register_native_function_param_default_array_inner(
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
pub(super) unsafe fn register_native_function_param_default_inner(
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
