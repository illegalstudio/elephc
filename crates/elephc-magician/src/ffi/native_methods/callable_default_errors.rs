//! Purpose:
//! Registers lazy native method and constructor parameter-default Errors.
//!
//! Called from:
//! - Generated eval metadata registration in the compiler backend.
//!
//! Key details:
//! - Messages are copied into context-owned signature metadata, not raised during registration.
//! - Invalid borrowed ABI buffers, context handles and parameter indexes fail closed.

use super::*;

/// Registers a lazy instance-method parameter Error.
///
/// # Safety
/// The context and both byte buffers must be valid for their declared lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_method_param_default_error(
    ctx: *mut ElephcEvalContext, key: *const u8, key_len: u64, index: u64,
    message: *const u8, message_len: u64,
) -> i32 {
    register_error(ctx, key, key_len, index, message, message_len, 0)
}

/// Registers a lazy static-method parameter Error.
///
/// # Safety
/// The context and both byte buffers must be valid for their declared lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_static_method_param_default_error(
    ctx: *mut ElephcEvalContext, key: *const u8, key_len: u64, index: u64,
    message: *const u8, message_len: u64,
) -> i32 {
    register_error(ctx, key, key_len, index, message, message_len, 1)
}

/// Registers a lazy constructor parameter Error.
///
/// # Safety
/// The context and both byte buffers must be valid for their declared lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_constructor_param_default_error(
    ctx: *mut ElephcEvalContext, key: *const u8, key_len: u64, index: u64,
    message: *const u8, message_len: u64,
) -> i32 {
    register_error(ctx, key, key_len, index, message, message_len, 2)
}

/// Applies the panic boundary and delegates to the existing signature validation paths.
///
/// # Safety
/// Mirrors the public entry points' borrowed context and byte-buffer requirements.
unsafe fn register_error(
    ctx: *mut ElephcEvalContext, key: *const u8, key_len: u64, index: u64,
    message: *const u8, message_len: u64, kind: u8,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        let Ok(message) = abi_name_to_string(message, message_len) else { return 0 };
        let default = NativeCallableDefault::DeferredError(message);
        if kind == 2 {
            register_native_constructor_param_default_inner(ctx, key, key_len, index, default)
        } else {
            register_native_method_param_default_inner(ctx, key, key_len, kind == 1, index, default)
        }
    }).unwrap_or(0)
}
