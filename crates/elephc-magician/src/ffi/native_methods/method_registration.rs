//! Purpose:
//! Validates and records generated method, interface-property, and abstract-
//! property metadata after the public ABI panic boundary.
//!
//! Called from:
//! - `super::public_abi` registration entry points.
//!
//! Key details:
//! - Invalid handles, names, types, defaults, or parameter indexes fail closed.

use super::*;

/// Runs native method registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_method`; invalid handles, names, or
/// counts fail closed as `false`.
pub(super) unsafe fn register_native_method_inner(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    is_static: bool,
    param_count: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(method_key) = abi_name_to_string(method_key_ptr, method_key_len) else {
        return 0;
    };
    let Some((class_name, method_name)) = split_method_key(&method_key) else {
        return 0;
    };
    let Ok(param_count) = usize::try_from(param_count) else {
        return 0;
    };
    let signature = NativeCallableSignature::new(param_count);
    if is_static {
        i32::from(context.define_native_static_method_signature(class_name, method_name, signature))
    } else {
        i32::from(context.define_native_method_signature(class_name, method_name, signature))
    }
}

/// Runs native interface property registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_interface_property`; invalid handles,
/// names, flags, or type specs fail closed as `false`.
pub(super) unsafe fn register_native_interface_property_inner(
    ctx: *mut ElephcEvalContext,
    property_key_ptr: *const u8,
    property_key_len: u64,
    type_spec_ptr: *const u8,
    type_spec_len: u64,
    flags: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(property_key) = abi_name_to_string(property_key_ptr, property_key_len) else {
        return 0;
    };
    let Some((interface_name, declaring_interface_name, property_name)) =
        split_three_part_property_key(&property_key)
    else {
        return 0;
    };
    let requires_get = flags & NATIVE_PROPERTY_REQUIRES_GET != 0;
    let requires_set = flags & NATIVE_PROPERTY_REQUIRES_SET != 0;
    if !requires_get && !requires_set {
        return 0;
    }
    let Some(property_type) = native_callable_type_from_abi(
        type_spec_ptr,
        type_spec_len,
        NativeCallableTypePosition::Parameter,
    ) else {
        return 0;
    };
    let property = EvalInterfaceProperty::new(property_name, requires_get, requires_set)
        .with_type(Some(property_type));
    i32::from(context.define_native_interface_property_requirement(
        interface_name,
        declaring_interface_name,
        property,
    ))
}

/// Runs native abstract property registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_abstract_property`; invalid handles,
/// names, flags, or type specs fail closed as `false`.
pub(super) unsafe fn register_native_abstract_property_inner(
    ctx: *mut ElephcEvalContext,
    property_key_ptr: *const u8,
    property_key_len: u64,
    type_spec_ptr: *const u8,
    type_spec_len: u64,
    flags: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(property_key) = abi_name_to_string(property_key_ptr, property_key_len) else {
        return 0;
    };
    let Some((class_name, declaring_class_name, property_name)) =
        split_three_part_property_key(&property_key)
    else {
        return 0;
    };
    let requires_get = flags & NATIVE_PROPERTY_REQUIRES_GET != 0;
    let requires_set = flags & NATIVE_PROPERTY_REQUIRES_SET != 0;
    if !requires_get && !requires_set {
        return 0;
    }
    let Some(property_type) = native_callable_type_from_abi(
        type_spec_ptr,
        type_spec_len,
        NativeCallableTypePosition::Parameter,
    ) else {
        return 0;
    };
    let property = EvalInterfaceProperty::new(property_name, requires_get, requires_set)
        .with_type(Some(property_type));
    i32::from(context.define_native_abstract_property_requirement(
        class_name,
        declaring_class_name,
        property,
    ))
}

/// Runs native method parameter registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_method_param`; invalid handles, names,
/// or indexes fail closed as `false`.
pub(super) unsafe fn register_native_method_param_inner(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    is_static: bool,
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
    let Ok(method_key) = abi_name_to_string(method_key_ptr, method_key_len) else {
        return 0;
    };
    let Some((class_name, method_name)) = split_method_key(&method_key) else {
        return 0;
    };
    let Ok(param_name) = abi_name_to_string(param_name_ptr, param_name_len) else {
        return 0;
    };
    let Ok(param_index) = usize::try_from(param_index) else {
        return 0;
    };
    if is_static {
        i32::from(context.define_native_static_method_param(
            class_name,
            method_name,
            param_index,
            param_name,
        ))
    } else {
        i32::from(context.define_native_method_param(
            class_name,
            method_name,
            param_index,
            param_name,
        ))
    }
}

/// Runs native method parameter-flag registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_method_param_flags`; invalid handles,
/// names, or indexes fail closed as `false`.
pub(super) unsafe fn register_native_method_param_flags_inner(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    is_static: bool,
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
    let Ok(method_key) = abi_name_to_string(method_key_ptr, method_key_len) else {
        return 0;
    };
    let Some((class_name, method_name)) = split_method_key(&method_key) else {
        return 0;
    };
    let Ok(param_index) = usize::try_from(param_index) else {
        return 0;
    };
    let by_ref_registered = if is_static {
        context.define_native_static_method_param_by_ref(
            class_name,
            method_name,
            param_index,
            is_by_ref != 0,
        )
    } else {
        context.define_native_method_param_by_ref(
            class_name,
            method_name,
            param_index,
            is_by_ref != 0,
        )
    };
    if !by_ref_registered {
        return 0;
    }
    if is_variadic == 0 {
        return 1;
    }
    i32::from(if is_static {
        context.define_native_static_method_variadic_param(class_name, method_name, param_index)
    } else {
        context.define_native_method_variadic_param(class_name, method_name, param_index)
    })
}

/// Runs native method bridge-support registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_method_bridge_support`; invalid
/// handles or names fail closed as `false`.
pub(super) unsafe fn register_native_method_bridge_support_inner(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    is_static: bool,
    supported: i32,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(method_key) = abi_name_to_string(method_key_ptr, method_key_len) else {
        return 0;
    };
    let Some((class_name, method_name)) = split_method_key(&method_key) else {
        return 0;
    };
    i32::from(if is_static {
        context.define_native_static_method_bridge_supported(
            class_name,
            method_name,
            supported != 0,
        )
    } else {
        context.define_native_method_bridge_supported(class_name, method_name, supported != 0)
    })
}

/// Runs native method parameter-type registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_method_param_type`; invalid handles,
/// names, indexes, or type specs fail closed as `false`.
pub(super) unsafe fn register_native_method_param_type_inner(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    is_static: bool,
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
    let Ok(method_key) = abi_name_to_string(method_key_ptr, method_key_len) else {
        return 0;
    };
    let Some((class_name, method_name)) = split_method_key(&method_key) else {
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
    if is_static {
        i32::from(context.define_native_static_method_param_type(
            class_name,
            method_name,
            param_index,
            param_type,
        ))
    } else {
        i32::from(context.define_native_method_param_type(
            class_name,
            method_name,
            param_index,
            param_type,
        ))
    }
}

/// Runs native method return-type registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_method_return_type`; invalid handles,
/// names, or type specs fail closed as `false`.
pub(super) unsafe fn register_native_method_return_type_inner(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    is_static: bool,
    type_spec_ptr: *const u8,
    type_spec_len: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(method_key) = abi_name_to_string(method_key_ptr, method_key_len) else {
        return 0;
    };
    let Some((class_name, method_name)) = split_method_key(&method_key) else {
        return 0;
    };
    let Some(return_type) = native_callable_type_from_abi(
        type_spec_ptr,
        type_spec_len,
        NativeCallableTypePosition::Return,
    ) else {
        return 0;
    };
    if is_static {
        i32::from(context.define_native_static_method_return_type(
            class_name,
            method_name,
            return_type,
        ))
    } else {
        i32::from(context.define_native_method_return_type(class_name, method_name, return_type))
    }
}

/// Runs native method scalar-default registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_method_param_default_scalar`; invalid
/// handles, names, indexes, or default kinds fail closed as `false`.
pub(super) unsafe fn register_native_method_param_default_scalar_inner(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    is_static: bool,
    param_index: u64,
    default_kind: u64,
    default_payload: u64,
) -> i32 {
    let Some(default) = native_callable_scalar_default(default_kind, default_payload) else {
        return 0;
    };
    register_native_method_param_default_inner(
        ctx,
        method_key_ptr,
        method_key_len,
        is_static,
        param_index,
        default,
    )
}

/// Runs native method string-default registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_method_param_default_string`; invalid
/// handles, names, or indexes fail closed as `false`.
pub(super) unsafe fn register_native_method_param_default_string_inner(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    is_static: bool,
    param_index: u64,
    default_ptr: *const u8,
    default_len: u64,
) -> i32 {
    let Ok(default) = abi_name_to_string(default_ptr, default_len) else {
        return 0;
    };
    register_native_method_param_default_inner(
        ctx,
        method_key_ptr,
        method_key_len,
        is_static,
        param_index,
        NativeCallableDefault::String(default),
    )
}

/// Runs native method object-default registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_method_param_default_object`; invalid
/// handles, names, indexes, or object specs fail closed as `false`.
pub(super) unsafe fn register_native_method_param_default_object_inner(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    is_static: bool,
    param_index: u64,
    spec_ptr: *const u8,
    spec_len: u64,
) -> i32 {
    let Some(default) = native_callable_object_default(spec_ptr, spec_len) else {
        return 0;
    };
    register_native_method_param_default_inner(
        ctx,
        method_key_ptr,
        method_key_len,
        is_static,
        param_index,
        default,
    )
}

/// Runs native method array-default registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_method_param_default_array`; invalid
/// handles, names, indexes, or array specs fail closed as `false`.
pub(super) unsafe fn register_native_method_param_default_array_inner(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    is_static: bool,
    param_index: u64,
    spec_ptr: *const u8,
    spec_len: u64,
) -> i32 {
    let Some(default) = native_callable_array_default(spec_ptr, spec_len) else {
        return 0;
    };
    register_native_method_param_default_inner(
        ctx,
        method_key_ptr,
        method_key_len,
        is_static,
        param_index,
        default,
    )
}

/// Records a native method parameter default in the selected instance/static table.
///
/// # Safety
/// `ctx` and `method_key_ptr` must be valid for their declared use; callers are
/// the exported ABI wrappers above.
pub(super) unsafe fn register_native_method_param_default_inner(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    is_static: bool,
    param_index: u64,
    default: NativeCallableDefault,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(method_key) = abi_name_to_string(method_key_ptr, method_key_len) else {
        return 0;
    };
    let Some((class_name, method_name)) = split_method_key(&method_key) else {
        return 0;
    };
    let Ok(param_index) = usize::try_from(param_index) else {
        return 0;
    };
    if is_static {
        i32::from(context.define_native_static_method_param_default(
            class_name,
            method_name,
            param_index,
            default,
        ))
    } else {
        i32::from(context.define_native_method_param_default(
            class_name,
            method_name,
            param_index,
            default,
        ))
    }
}
