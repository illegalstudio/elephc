//! Purpose:
//! Validates and records generated constructor signatures, parameter metadata,
//! bridge support, and default values.
//!
//! Called from:
//! - `super::public_abi` constructor registration entry points.
//!
//! Key details:
//! - Registration rejects malformed names, unsupported defaults, and indexes
//!   outside the declared constructor signature.

use super::*;

/// Runs native constructor registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_constructor`; invalid handles, names,
/// or counts fail closed as `false`.
pub(super) unsafe fn register_native_constructor_inner(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    param_count: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(class_name) = abi_name_to_string(class_name_ptr, class_name_len) else {
        return 0;
    };
    let Ok(param_count) = usize::try_from(param_count) else {
        return 0;
    };
    i32::from(context.define_native_constructor_signature(
        &class_name,
        NativeCallableSignature::new(param_count),
    ))
}

/// Runs native constructor parameter registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_constructor_param`; invalid handles,
/// names, or indexes fail closed as `false`.
pub(super) unsafe fn register_native_constructor_param_inner(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
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
    let Ok(class_name) = abi_name_to_string(class_name_ptr, class_name_len) else {
        return 0;
    };
    let Ok(param_name) = abi_name_to_string(param_name_ptr, param_name_len) else {
        return 0;
    };
    let Ok(param_index) = usize::try_from(param_index) else {
        return 0;
    };
    i32::from(context.define_native_constructor_param(&class_name, param_index, param_name))
}

/// Runs native constructor parameter-flag registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_constructor_param_flags`; invalid
/// handles, names, or indexes fail closed as `false`.
pub(super) unsafe fn register_native_constructor_param_flags_inner(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
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
    let Ok(class_name) = abi_name_to_string(class_name_ptr, class_name_len) else {
        return 0;
    };
    let Ok(param_index) = usize::try_from(param_index) else {
        return 0;
    };
    if !context.define_native_constructor_param_by_ref(&class_name, param_index, is_by_ref != 0) {
        return 0;
    }
    if is_variadic == 0 {
        return 1;
    }
    i32::from(context.define_native_constructor_variadic_param(&class_name, param_index))
}

/// Runs native constructor bridge-support registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_constructor_bridge_support`; invalid
/// handles or names fail closed as `false`.
pub(super) unsafe fn register_native_constructor_bridge_support_inner(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    supported: i32,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(class_name) = abi_name_to_string(class_name_ptr, class_name_len) else {
        return 0;
    };
    i32::from(context.define_native_constructor_bridge_supported(&class_name, supported != 0))
}

/// Runs native constructor parameter-type registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_constructor_param_type`; invalid
/// handles, names, indexes, or type specs fail closed as `false`.
pub(super) unsafe fn register_native_constructor_param_type_inner(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
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
    let Ok(class_name) = abi_name_to_string(class_name_ptr, class_name_len) else {
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
    i32::from(context.define_native_constructor_param_type(&class_name, param_index, param_type))
}

/// Runs native constructor scalar-default registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_constructor_param_default_scalar`;
/// invalid handles, names, indexes, or default kinds fail closed as `false`.
pub(super) unsafe fn register_native_constructor_param_default_scalar_inner(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    param_index: u64,
    default_kind: u64,
    default_payload: u64,
) -> i32 {
    let Some(default) = native_callable_scalar_default(default_kind, default_payload) else {
        return 0;
    };
    register_native_constructor_param_default_inner(
        ctx,
        class_name_ptr,
        class_name_len,
        param_index,
        default,
    )
}

/// Runs native constructor string-default registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_constructor_param_default_string`;
/// invalid handles, names, or indexes fail closed as `false`.
pub(super) unsafe fn register_native_constructor_param_default_string_inner(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    param_index: u64,
    default_ptr: *const u8,
    default_len: u64,
) -> i32 {
    let Ok(default) = abi_name_to_string(default_ptr, default_len) else {
        return 0;
    };
    register_native_constructor_param_default_inner(
        ctx,
        class_name_ptr,
        class_name_len,
        param_index,
        NativeCallableDefault::String(default),
    )
}

/// Runs native constructor object-default registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_constructor_param_default_object`;
/// invalid handles, names, indexes, or object specs fail closed as `false`.
pub(super) unsafe fn register_native_constructor_param_default_object_inner(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    param_index: u64,
    spec_ptr: *const u8,
    spec_len: u64,
) -> i32 {
    let Some(default) = native_callable_object_default(spec_ptr, spec_len) else {
        return 0;
    };
    register_native_constructor_param_default_inner(
        ctx,
        class_name_ptr,
        class_name_len,
        param_index,
        default,
    )
}

/// Runs native constructor array-default registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_constructor_param_default_array`;
/// invalid handles, names, indexes, or array specs fail closed as `false`.
pub(super) unsafe fn register_native_constructor_param_default_array_inner(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    param_index: u64,
    spec_ptr: *const u8,
    spec_len: u64,
) -> i32 {
    let Some(default) = native_callable_array_default(spec_ptr, spec_len) else {
        return 0;
    };
    register_native_constructor_param_default_inner(
        ctx,
        class_name_ptr,
        class_name_len,
        param_index,
        default,
    )
}

/// Records a native constructor parameter default in the constructor signature table.
///
/// # Safety
/// `ctx` and `class_name_ptr` must be valid for their declared use; callers are
/// the exported ABI wrappers above.
pub(super) unsafe fn register_native_constructor_param_default_inner(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    param_index: u64,
    default: NativeCallableDefault,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(class_name) = abi_name_to_string(class_name_ptr, class_name_len) else {
        return 0;
    };
    let Ok(param_index) = usize::try_from(param_index) else {
        return 0;
    };
    i32::from(context.define_native_constructor_param_default(&class_name, param_index, default))
}
