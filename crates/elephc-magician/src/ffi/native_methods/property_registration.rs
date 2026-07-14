//! Purpose:
//! Validates and records native class parents, property contracts and defaults,
//! and generated class-member attributes.
//!
//! Called from:
//! - `super::public_abi` property and class metadata registration entry points.
//!
//! Key details:
//! - Property keys are decoded into their declaring class-like components before
//!   context mutation.

use super::*;

/// Runs native parent-class registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_class_parent`; invalid handles or
/// names fail closed as `false`.
pub(super) unsafe fn register_native_class_parent_inner(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    parent_name_ptr: *const u8,
    parent_name_len: u64,
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
    let Ok(parent_name) = abi_name_to_string(parent_name_ptr, parent_name_len) else {
        return 0;
    };
    i32::from(context.define_native_class_parent(&class_name, &parent_name))
}

/// Runs native property-type registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_property_type`; invalid handles,
/// names, or type specs fail closed as `false`.
pub(super) unsafe fn register_native_property_type_inner(
    ctx: *mut ElephcEvalContext,
    property_key_ptr: *const u8,
    property_key_len: u64,
    type_spec_ptr: *const u8,
    type_spec_len: u64,
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
    let Some((class_name, property_name)) = split_property_key(&property_key) else {
        return 0;
    };
    let Some(property_type) = native_callable_type_from_abi(
        type_spec_ptr,
        type_spec_len,
        NativeCallableTypePosition::Parameter,
    ) else {
        return 0;
    };
    i32::from(context.define_native_property_type(class_name, property_name, property_type))
}

/// Runs native property scalar-default registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_property_default_scalar`; invalid
/// handles, names, or default kinds fail closed as `false`.
pub(super) unsafe fn register_native_property_default_scalar_inner(
    ctx: *mut ElephcEvalContext,
    property_key_ptr: *const u8,
    property_key_len: u64,
    default_kind: u64,
    default_payload: u64,
) -> i32 {
    let Some(default) = native_callable_scalar_default(default_kind, default_payload) else {
        return 0;
    };
    register_native_property_default_inner(ctx, property_key_ptr, property_key_len, default)
}

/// Runs native property string-default registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_property_default_string`; invalid
/// handles, names, or string buffers fail closed as `false`.
pub(super) unsafe fn register_native_property_default_string_inner(
    ctx: *mut ElephcEvalContext,
    property_key_ptr: *const u8,
    property_key_len: u64,
    default_ptr: *const u8,
    default_len: u64,
) -> i32 {
    let Ok(default) = abi_name_to_string(default_ptr, default_len) else {
        return 0;
    };
    register_native_property_default_inner(
        ctx,
        property_key_ptr,
        property_key_len,
        NativeCallableDefault::String(default),
    )
}

/// Runs native property array-default registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_property_default_array`; invalid
/// handles, names, or array specs fail closed as `false`.
pub(super) unsafe fn register_native_property_default_array_inner(
    ctx: *mut ElephcEvalContext,
    property_key_ptr: *const u8,
    property_key_len: u64,
    spec_ptr: *const u8,
    spec_len: u64,
) -> i32 {
    let Some(default) = native_callable_array_default(spec_ptr, spec_len) else {
        return 0;
    };
    register_native_property_default_inner(ctx, property_key_ptr, property_key_len, default)
}

/// Records a native property default in the property metadata table.
///
/// # Safety
/// `ctx` and `property_key_ptr` must be valid for their declared use; callers
/// are the exported ABI wrappers above.
pub(super) unsafe fn register_native_property_default_inner(
    ctx: *mut ElephcEvalContext,
    property_key_ptr: *const u8,
    property_key_len: u64,
    default: NativeCallableDefault,
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
    let Some((class_name, property_name)) = split_property_key(&property_key) else {
        return 0;
    };
    i32::from(context.define_native_property_default(class_name, property_name, default))
}

/// Runs native member-attribute registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_member_attribute`; invalid handles or
/// binary records fail closed as `false`.
pub(super) unsafe fn register_native_member_attribute_inner(
    ctx: *mut ElephcEvalContext,
    record_ptr: *const u8,
    record_len: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Some(record) = native_member_attribute_record_from_abi(record_ptr, record_len) else {
        return 0;
    };
    match record.owner_kind {
        NATIVE_MEMBER_ATTRIBUTE_CLASS => {
            i32::from(context.define_native_class_attribute(&record.member_key, record.attribute))
        }
        NATIVE_MEMBER_ATTRIBUTE_METHOD
        | NATIVE_MEMBER_ATTRIBUTE_PROPERTY
        | NATIVE_MEMBER_ATTRIBUTE_CLASS_CONSTANT => {
            let Some((class_name, member_name)) = split_method_key(&record.member_key) else {
                return 0;
            };
            match record.owner_kind {
                NATIVE_MEMBER_ATTRIBUTE_METHOD => i32::from(context.define_native_method_attribute(
                    class_name,
                    member_name,
                    record.attribute,
                )),
                NATIVE_MEMBER_ATTRIBUTE_PROPERTY => i32::from(
                    context.define_native_property_attribute(class_name, member_name, record.attribute),
                ),
                NATIVE_MEMBER_ATTRIBUTE_CLASS_CONSTANT => {
                    i32::from(context.define_native_constant_attribute(
                        class_name,
                        member_name,
                        record.attribute,
                    ))
                }
                _ => 0,
            }
        }
        _ => 0,
    }
}
