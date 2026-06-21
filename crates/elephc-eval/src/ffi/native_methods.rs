//! Purpose:
//! Exports registration of generated native PHP method signatures into an eval
//! context so runtime fragments can bind AOT method named arguments.
//!
//! Called from:
//! - Generated EIR backend assembly before fragments can call AOT methods.
//!
//! Key details:
//! - Invalid names, handles, or indexes fail closed as `false`.
//! - The metadata records parameter names and scalar defaults; generated user
//!   helpers still perform the actual method, static method, and constructor calls.

use super::util::abi_name_to_string;
use crate::abi::{ElephcEvalContext, ABI_VERSION};
use crate::context::{NativeCallableDefault, NativeCallableSignature};
use crate::eval_ir::{EvalParameterType, EvalParameterTypeVariant};

const NATIVE_DEFAULT_NULL: u64 = 0;
const NATIVE_DEFAULT_BOOL: u64 = 1;
const NATIVE_DEFAULT_INT: u64 = 2;
const NATIVE_DEFAULT_FLOAT: u64 = 3;

#[derive(Clone, Copy)]
enum NativeCallableTypePosition {
    Parameter,
    Return,
}

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

/// Runs native method registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_method`; invalid handles, names, or
/// counts fail closed as `false`.
unsafe fn register_native_method_inner(
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

/// Runs native method parameter registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_method_param`; invalid handles, names,
/// or indexes fail closed as `false`.
unsafe fn register_native_method_param_inner(
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

/// Runs native method parameter-type registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_method_param_type`; invalid handles,
/// names, indexes, or type specs fail closed as `false`.
unsafe fn register_native_method_param_type_inner(
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
unsafe fn register_native_method_return_type_inner(
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
unsafe fn register_native_method_param_default_scalar_inner(
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
unsafe fn register_native_method_param_default_string_inner(
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

/// Records a native method parameter default in the selected instance/static table.
///
/// # Safety
/// `ctx` and `method_key_ptr` must be valid for their declared use; callers are
/// the exported ABI wrappers above.
unsafe fn register_native_method_param_default_inner(
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

/// Runs native constructor registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_constructor`; invalid handles, names,
/// or counts fail closed as `false`.
unsafe fn register_native_constructor_inner(
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
unsafe fn register_native_constructor_param_inner(
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

/// Runs native constructor parameter-type registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_constructor_param_type`; invalid
/// handles, names, indexes, or type specs fail closed as `false`.
unsafe fn register_native_constructor_param_type_inner(
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
unsafe fn register_native_constructor_param_default_scalar_inner(
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
unsafe fn register_native_constructor_param_default_string_inner(
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

/// Records a native constructor parameter default in the constructor signature table.
///
/// # Safety
/// `ctx` and `class_name_ptr` must be valid for their declared use; callers are
/// the exported ABI wrappers above.
unsafe fn register_native_constructor_param_default_inner(
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

/// Runs native parent-class registration after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_class_parent`; invalid handles or
/// names fail closed as `false`.
unsafe fn register_native_class_parent_inner(
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
unsafe fn register_native_property_type_inner(
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
unsafe fn register_native_property_default_scalar_inner(
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
unsafe fn register_native_property_default_string_inner(
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

/// Records a native property default in the property metadata table.
///
/// # Safety
/// `ctx` and `property_key_ptr` must be valid for their declared use; callers
/// are the exported ABI wrappers above.
unsafe fn register_native_property_default_inner(
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

/// Decodes scalar default kind/payload ABI fields into native callable metadata.
fn native_callable_scalar_default(
    default_kind: u64,
    default_payload: u64,
) -> Option<NativeCallableDefault> {
    match default_kind {
        NATIVE_DEFAULT_NULL => Some(NativeCallableDefault::Null),
        NATIVE_DEFAULT_BOOL => Some(NativeCallableDefault::Bool(default_payload != 0)),
        NATIVE_DEFAULT_INT => Some(NativeCallableDefault::Int(default_payload as i64)),
        NATIVE_DEFAULT_FLOAT => Some(NativeCallableDefault::Float(f64::from_bits(
            default_payload,
        ))),
        _ => None,
    }
}

/// Decodes one generated type-spec string into eval Reflection type metadata.
fn native_callable_type_from_abi(
    type_spec_ptr: *const u8,
    type_spec_len: u64,
    position: NativeCallableTypePosition,
) -> Option<EvalParameterType> {
    let type_spec = abi_name_to_string(type_spec_ptr, type_spec_len).ok()?;
    native_callable_type_from_spec(&type_spec, position)
}

/// Parses the compact generated type syntax used by native signature registration.
fn native_callable_type_from_spec(
    type_spec: &str,
    position: NativeCallableTypePosition,
) -> Option<EvalParameterType> {
    let type_spec = type_spec.trim();
    if type_spec.is_empty() {
        return None;
    }
    let nullable_shorthand = type_spec.strip_prefix('?');
    let (type_spec, mut allows_null) = match nullable_shorthand {
        Some(inner) => (inner, true),
        None => (type_spec, false),
    };
    if type_spec.contains('&') {
        if allows_null || type_spec.contains('|') {
            return None;
        }
        let variants = type_spec
            .split('&')
            .map(|member| native_callable_type_variant(member, position))
            .collect::<Option<Vec<_>>>()?;
        if variants.iter().any(Option::is_none) {
            return None;
        }
        return Some(EvalParameterType::intersection(
            variants.into_iter().flatten().collect(),
        ));
    }
    let mut variants = Vec::new();
    for member in type_spec.split('|') {
        match native_callable_type_variant(member, position)? {
            Some(variant) => variants.push(variant),
            None => allows_null = true,
        }
    }
    if variants.is_empty() {
        return None;
    }
    Some(EvalParameterType::new(variants, allows_null))
}

/// Converts one generated type member name into eval type metadata.
fn native_callable_type_variant(
    member: &str,
    position: NativeCallableTypePosition,
) -> Option<Option<EvalParameterTypeVariant>> {
    let member = member.trim();
    if member.is_empty() {
        return None;
    }
    let lower = member.trim_start_matches('\\').to_ascii_lowercase();
    let variant = match lower.as_str() {
        "array" => EvalParameterTypeVariant::Array,
        "bool" => EvalParameterTypeVariant::Bool,
        "callable" => EvalParameterTypeVariant::Callable,
        "float" => EvalParameterTypeVariant::Float,
        "int" => EvalParameterTypeVariant::Int,
        "iterable" => EvalParameterTypeVariant::Iterable,
        "mixed" => EvalParameterTypeVariant::Mixed,
        "never" if matches!(position, NativeCallableTypePosition::Return) => {
            EvalParameterTypeVariant::Never
        }
        "null" => return Some(None),
        "object" => EvalParameterTypeVariant::Object,
        "string" => EvalParameterTypeVariant::String,
        "void" if matches!(position, NativeCallableTypePosition::Return) => {
            EvalParameterTypeVariant::Void
        }
        "void" | "never" => return None,
        "self" | "parent" | "static" => EvalParameterTypeVariant::Class(lower),
        _ => EvalParameterTypeVariant::Class(member.trim_start_matches('\\').to_string()),
    };
    Some(Some(variant))
}

/// Splits one generated `ClassName::methodName` metadata key into class and method pieces.
fn split_method_key(method_key: &str) -> Option<(&str, &str)> {
    let (class_name, method_name) = method_key.rsplit_once("::")?;
    (!class_name.is_empty() && !method_name.is_empty()).then_some((class_name, method_name))
}

/// Splits one generated `ClassName::propertyName` metadata key into class and property pieces.
fn split_property_key(property_key: &str) -> Option<(&str, &str)> {
    split_method_key(property_key)
}
