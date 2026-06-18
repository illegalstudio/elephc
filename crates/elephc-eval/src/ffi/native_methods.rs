//! Purpose:
//! Exports registration of generated native PHP method signatures into an eval
//! context so runtime fragments can bind AOT method named arguments.
//!
//! Called from:
//! - Generated EIR backend assembly before fragments can call AOT methods.
//!
//! Key details:
//! - Invalid names, handles, or indexes fail closed as `false`.
//! - The metadata records parameter names only; generated user helpers still
//!   perform the actual method, static method, and constructor calls.

use super::util::abi_name_to_string;
use crate::abi::{ElephcEvalContext, ABI_VERSION};
use crate::context::NativeCallableSignature;

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
        i32::from(context.define_native_static_method_signature(
            class_name,
            method_name,
            signature,
        ))
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

/// Splits one generated `ClassName::methodName` metadata key into class and method pieces.
fn split_method_key(method_key: &str) -> Option<(&str, &str)> {
    let (class_name, method_name) = method_key.rsplit_once("::")?;
    (!class_name.is_empty() && !method_name.is_empty()).then_some((class_name, method_name))
}
