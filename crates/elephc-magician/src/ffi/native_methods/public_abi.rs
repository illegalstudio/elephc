//! Purpose:
//! Exposes the C ABI entry points that register generated method, constructor,
//! property, interface, and attribute metadata in an eval context.
//!
//! Called from:
//! - Generated EIR backend assembly during native fragment registration.
//!
//! Key details:
//! - Every entry point installs a panic boundary and delegates validation to a
//!   focused registration helper.

use super::*;

/// Registers a generated native PHP method signature in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `method_key_ptr` must point to a
/// readable `ClassName::methodName` byte string.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_method(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    param_count: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_inner(ctx, method_key_ptr, method_key_len, false, param_count)
    })
    .unwrap_or(0)
}

/// Registers a generated native PHP static-method signature in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `method_key_ptr` must point to a
/// readable `ClassName::methodName` byte string.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_static_method(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    param_count: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_inner(ctx, method_key_ptr, method_key_len, true, param_count)
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP interface property contract in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `property_key_ptr` must point to a
/// readable `InterfaceName::DeclaringInterface::propertyName` byte string, and
/// `type_spec_ptr` must point to a readable generated type-spec byte string.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_interface_property(
    ctx: *mut ElephcEvalContext,
    property_key_ptr: *const u8,
    property_key_len: u64,
    type_spec_ptr: *const u8,
    type_spec_len: u64,
    flags: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_interface_property_inner(
            ctx,
            property_key_ptr,
            property_key_len,
            type_spec_ptr,
            type_spec_len,
            flags,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP abstract class property contract in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `property_key_ptr` must point to a
/// readable `ClassName::DeclaringClass::propertyName` byte string, and
/// `type_spec_ptr` must point to a readable generated type-spec byte string.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_abstract_property(
    ctx: *mut ElephcEvalContext,
    property_key_ptr: *const u8,
    property_key_len: u64,
    type_spec_ptr: *const u8,
    type_spec_len: u64,
    flags: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_abstract_property_inner(
            ctx,
            property_key_ptr,
            property_key_len,
            type_spec_ptr,
            type_spec_len,
            flags,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP method parameter name in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Method key and parameter name
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_method_param(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    param_index: u64,
    param_name_ptr: *const u8,
    param_name_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_param_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            false,
            param_index,
            param_name_ptr,
            param_name_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP static-method parameter name in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Method key and parameter name
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_static_method_param(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    param_index: u64,
    param_name_ptr: *const u8,
    param_name_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_param_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            true,
            param_index,
            param_name_ptr,
            param_name_len,
        )
    })
    .unwrap_or(0)
}

/// Registers generated native PHP method parameter flags in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. The method key must be readable
/// for its declared byte length.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_method_param_flags(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    param_index: u64,
    is_by_ref: i32,
    is_variadic: i32,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_param_flags_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            false,
            param_index,
            is_by_ref,
            is_variadic,
        )
    })
    .unwrap_or(0)
}

/// Registers generated native PHP static-method parameter flags in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. The method key must be readable
/// for its declared byte length.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_static_method_param_flags(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    param_index: u64,
    is_by_ref: i32,
    is_variadic: i32,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_param_flags_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            true,
            param_index,
            is_by_ref,
            is_variadic,
        )
    })
    .unwrap_or(0)
}

/// Registers whether generated eval may dispatch a native PHP method.
///
/// # Safety
/// `ctx` must be a valid eval context handle. The method key must be readable
/// for its declared byte length.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_method_bridge_support(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    supported: i32,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_bridge_support_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            false,
            supported,
        )
    })
    .unwrap_or(0)
}

/// Registers whether generated eval may dispatch a native PHP static method.
///
/// # Safety
/// `ctx` must be a valid eval context handle. The method key must be readable
/// for its declared byte length.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_static_method_bridge_support(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    supported: i32,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_bridge_support_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            true,
            supported,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP method parameter declared type in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Method key and type-spec pointers
/// must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_method_param_type(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    param_index: u64,
    type_spec_ptr: *const u8,
    type_spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_param_type_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            false,
            param_index,
            type_spec_ptr,
            type_spec_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP static-method parameter declared type.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Method key and type-spec pointers
/// must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_static_method_param_type(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    param_index: u64,
    type_spec_ptr: *const u8,
    type_spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_param_type_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            true,
            param_index,
            type_spec_ptr,
            type_spec_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP method declared return type in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Method key and type-spec pointers
/// must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_method_return_type(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    type_spec_ptr: *const u8,
    type_spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_return_type_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            false,
            type_spec_ptr,
            type_spec_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP static-method declared return type.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Method key and type-spec pointers
/// must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_static_method_return_type(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    type_spec_ptr: *const u8,
    type_spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_return_type_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            true,
            type_spec_ptr,
            type_spec_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP method scalar parameter default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Method key must be readable for
/// its declared byte length.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_method_param_default_scalar(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    param_index: u64,
    default_kind: u64,
    default_payload: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_param_default_scalar_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            false,
            param_index,
            default_kind,
            default_payload,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP static-method scalar parameter default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Method key must be readable for
/// its declared byte length.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_static_method_param_default_scalar(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    param_index: u64,
    default_kind: u64,
    default_payload: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_param_default_scalar_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            true,
            param_index,
            default_kind,
            default_payload,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP method string parameter default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Method key and default string
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_method_param_default_string(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    param_index: u64,
    default_ptr: *const u8,
    default_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_param_default_string_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            false,
            param_index,
            default_ptr,
            default_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP static-method string parameter default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Method key and default string
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_static_method_param_default_string(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    param_index: u64,
    default_ptr: *const u8,
    default_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_param_default_string_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            true,
            param_index,
            default_ptr,
            default_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP method object parameter default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Method key and encoded default
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_method_param_default_object(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    param_index: u64,
    spec_ptr: *const u8,
    spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_param_default_object_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            false,
            param_index,
            spec_ptr,
            spec_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP static-method object parameter default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Method key and encoded default
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_static_method_param_default_object(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    param_index: u64,
    spec_ptr: *const u8,
    spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_param_default_object_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            true,
            param_index,
            spec_ptr,
            spec_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP method array parameter default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Method key and encoded default
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_method_param_default_array(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    param_index: u64,
    spec_ptr: *const u8,
    spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_param_default_array_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            false,
            param_index,
            spec_ptr,
            spec_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP static-method array parameter default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Method key and encoded default
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_static_method_param_default_array(
    ctx: *mut ElephcEvalContext,
    method_key_ptr: *const u8,
    method_key_len: u64,
    param_index: u64,
    spec_ptr: *const u8,
    spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_method_param_default_array_inner(
            ctx,
            method_key_ptr,
            method_key_len,
            true,
            param_index,
            spec_ptr,
            spec_len,
        )
    })
    .unwrap_or(0)
}

/// Registers a generated native PHP constructor signature in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `class_name_ptr` must be readable
/// for `class_name_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_constructor(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    param_count: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_constructor_inner(ctx, class_name_ptr, class_name_len, param_count)
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP constructor parameter name in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Class and parameter name pointers
/// must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_constructor_param(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    param_index: u64,
    param_name_ptr: *const u8,
    param_name_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_constructor_param_inner(
            ctx,
            class_name_ptr,
            class_name_len,
            param_index,
            param_name_ptr,
            param_name_len,
        )
    })
    .unwrap_or(0)
}

/// Registers generated native PHP constructor parameter flags in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. The class name must be readable
/// for its declared byte length.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_constructor_param_flags(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    param_index: u64,
    is_by_ref: i32,
    is_variadic: i32,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_constructor_param_flags_inner(
            ctx,
            class_name_ptr,
            class_name_len,
            param_index,
            is_by_ref,
            is_variadic,
        )
    })
    .unwrap_or(0)
}

/// Registers whether generated eval may dispatch a native PHP constructor.
///
/// # Safety
/// `ctx` must be a valid eval context handle. The class name must be readable
/// for its declared byte length.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_constructor_bridge_support(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    supported: i32,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_constructor_bridge_support_inner(
            ctx,
            class_name_ptr,
            class_name_len,
            supported,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP constructor parameter declared type.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Class and type-spec pointers must
/// be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_constructor_param_type(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    param_index: u64,
    type_spec_ptr: *const u8,
    type_spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_constructor_param_type_inner(
            ctx,
            class_name_ptr,
            class_name_len,
            param_index,
            type_spec_ptr,
            type_spec_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP constructor scalar parameter default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Class name must be readable for
/// its declared byte length.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_constructor_param_default_scalar(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    param_index: u64,
    default_kind: u64,
    default_payload: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_constructor_param_default_scalar_inner(
            ctx,
            class_name_ptr,
            class_name_len,
            param_index,
            default_kind,
            default_payload,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP constructor string parameter default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Class name and default string
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_constructor_param_default_string(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    param_index: u64,
    default_ptr: *const u8,
    default_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_constructor_param_default_string_inner(
            ctx,
            class_name_ptr,
            class_name_len,
            param_index,
            default_ptr,
            default_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP constructor object parameter default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Class name and encoded default
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_constructor_param_default_object(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    param_index: u64,
    spec_ptr: *const u8,
    spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_constructor_param_default_object_inner(
            ctx,
            class_name_ptr,
            class_name_len,
            param_index,
            spec_ptr,
            spec_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP constructor array parameter default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Class name and encoded default
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_constructor_param_default_array(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    param_index: u64,
    spec_ptr: *const u8,
    spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_constructor_param_default_array_inner(
            ctx,
            class_name_ptr,
            class_name_len,
            param_index,
            spec_ptr,
            spec_len,
        )
    })
    .unwrap_or(0)
}

/// Registers generated native PHP parent-class metadata in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Class and parent name pointers
/// must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_class_parent(
    ctx: *mut ElephcEvalContext,
    class_name_ptr: *const u8,
    class_name_len: u64,
    parent_name_ptr: *const u8,
    parent_name_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_class_parent_inner(
            ctx,
            class_name_ptr,
            class_name_len,
            parent_name_ptr,
            parent_name_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP property type in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. The property key must be a
/// readable `ClassName::propertyName` byte string, and the type spec must be a
/// readable generated type-spec byte string.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_property_type(
    ctx: *mut ElephcEvalContext,
    property_key_ptr: *const u8,
    property_key_len: u64,
    type_spec_ptr: *const u8,
    type_spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_property_type_inner(
            ctx,
            property_key_ptr,
            property_key_len,
            type_spec_ptr,
            type_spec_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP property scalar default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. The property key must be a
/// readable `ClassName::propertyName` byte string.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_property_default_scalar(
    ctx: *mut ElephcEvalContext,
    property_key_ptr: *const u8,
    property_key_len: u64,
    default_kind: u64,
    default_payload: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_property_default_scalar_inner(
            ctx,
            property_key_ptr,
            property_key_len,
            default_kind,
            default_payload,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP property string default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. The property key and default
/// string pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_property_default_string(
    ctx: *mut ElephcEvalContext,
    property_key_ptr: *const u8,
    property_key_len: u64,
    default_ptr: *const u8,
    default_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_property_default_string_inner(
            ctx,
            property_key_ptr,
            property_key_len,
            default_ptr,
            default_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP property array default in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. The property key and encoded
/// default pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_property_default_array(
    ctx: *mut ElephcEvalContext,
    property_key_ptr: *const u8,
    property_key_len: u64,
    spec_ptr: *const u8,
    spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_property_default_array_inner(
            ctx,
            property_key_ptr,
            property_key_len,
            spec_ptr,
            spec_len,
        )
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP class/member attribute in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `record_ptr` must point to one
/// readable binary member-attribute metadata record.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_member_attribute(
    ctx: *mut ElephcEvalContext,
    record_ptr: *const u8,
    record_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_member_attribute_inner(ctx, record_ptr, record_len)
    })
    .unwrap_or(0)
}
