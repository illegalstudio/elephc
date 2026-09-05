//! Purpose:
//! Registers AOT global constant metadata with a persistent eval context.
//!
//! Called from:
//! - Generated EIR backend assembly during eval context initialization.
//!
//! Key details:
//! - Scalar payloads cross the ABI as integer words or borrowed UTF-8 bytes.
//! - Invalid handles, kinds, names, and string storage fail closed as `false`.

use super::util::abi_name_to_string;
use crate::abi::{ElephcEvalContext, ABI_VERSION};
use crate::context::EvalNativeGlobalConstant;

const NATIVE_CONSTANT_NULL: u64 = 0;
const NATIVE_CONSTANT_BOOL: u64 = 1;
const NATIVE_CONSTANT_INT: u64 = 2;
const NATIVE_CONSTANT_FLOAT: u64 = 3;
const NATIVE_CONSTANT_STRING: u64 = 4;
const NATIVE_CONSTANT_RESOURCE: u64 = 5;

/// Registers one AOT scalar global constant for eval lookup and introspection.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Name and string pointers must be
/// readable for their corresponding lengths when those lengths are nonzero.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_global_constant(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    kind: u64,
    value_word: u64,
    value_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_global_constant_inner(
            ctx, name_ptr, name_len, kind, value_word, value_len,
        )
    })
    .unwrap_or(0)
}

/// Validates and stores one scalar global constant after the ABI panic boundary.
///
/// # Safety
/// Mirrors the exported registration function's pointer requirements.
unsafe fn register_native_global_constant_inner(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    kind: u64,
    value_word: u64,
    value_len: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return 0;
    };
    let value = match kind {
        NATIVE_CONSTANT_NULL => EvalNativeGlobalConstant::Null,
        NATIVE_CONSTANT_BOOL => EvalNativeGlobalConstant::Bool(value_word != 0),
        NATIVE_CONSTANT_INT => EvalNativeGlobalConstant::Int(value_word as i64),
        NATIVE_CONSTANT_FLOAT => EvalNativeGlobalConstant::Float(f64::from_bits(value_word)),
        NATIVE_CONSTANT_STRING => {
            let Ok(value) = abi_name_to_string(value_word as *const u8, value_len) else {
                return 0;
            };
            EvalNativeGlobalConstant::String(value)
        }
        NATIVE_CONSTANT_RESOURCE => EvalNativeGlobalConstant::Resource(value_word as i64),
        _ => return 0,
    };
    i32::from(context.define_native_global_constant(&name, value))
}
