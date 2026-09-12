//! Purpose:
//! Registers AOT global constant metadata with a persistent eval context.
//!
//! Called from:
//! - Generated EIR backend assembly during eval context initialization.
//!
//! Key details:
//! - Scalar payloads cross the ABI as integer words or borrowed UTF-8 bytes.
//! - Core and user constants live in separate context registries so PHP's
//!   `Core` / `user` categories stay exact; neither joins the dynamic
//!   `define()` map that the native constant inventory exports back to AOT.
//! - Array-valued user constants reuse the existing native callable
//!   array-default binary spec and its strict decoder, so there is no second
//!   compound codec to keep in sync; the context ABI version gates the call.
//! - The shared decoder also accepts object-valued elements, which only
//!   parameter defaults can materialize. User constants therefore re-validate
//!   the decoded element tree and reject object values at registration time
//!   instead of failing later, when the value is fetched.
//! - Invalid handles, kinds, names, and string storage fail closed as `false`.

use super::native_methods::native_callable_array_default;
use super::util::abi_name_to_string;
use crate::abi::{ElephcEvalContext, ABI_VERSION};
use crate::context::{
    EvalNativeGlobalConstant, EvalNativeUserConstant, NativeCallableArrayDefaultElement,
    NativeCallableDefault,
};

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

/// Registers one AOT scalar user-declared constant for eval lookup and introspection.
///
/// Shares the scalar kind/payload encoding with
/// `__elephc_eval_register_native_global_constant`; only the destination registry, and
/// therefore the reported PHP category, differs.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Name and string pointers must be
/// readable for their corresponding lengths when those lengths are nonzero.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_user_constant(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    kind: u64,
    value_word: u64,
    value_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_user_constant_inner(ctx, name_ptr, name_len, kind, value_word, value_len)
    })
    .unwrap_or(0)
}

/// Registers one AOT array-valued user-declared constant for eval lookup and introspection.
///
/// `spec_ptr` holds the same binary array spec the native callable array-default
/// registrations use. The shared decoder rejects truncated records, trailing bytes, and
/// unknown element or key tags, so a malformed blob fails closed instead of registering a
/// partial array. Object-valued elements decode successfully but are not a constant value
/// AOT can produce, so they are rejected here, at any nesting depth, before storing.
///
/// # Safety
/// `ctx` must be a valid eval context handle. The name and spec pointers must be
/// readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_user_constant_array(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    spec_ptr: *const u8,
    spec_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_user_constant_array_inner(ctx, name_ptr, name_len, spec_ptr, spec_len)
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
    let Some(value) = native_scalar_constant_value(kind, value_word, value_len) else {
        return 0;
    };
    i32::from(context.define_native_global_constant(&name, value))
}

/// Validates and stores one scalar user constant after the ABI panic boundary.
///
/// # Safety
/// Mirrors the exported registration function's pointer requirements.
unsafe fn register_native_user_constant_inner(
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
    let Some(value) = native_scalar_constant_value(kind, value_word, value_len) else {
        return 0;
    };
    i32::from(context.define_native_user_constant(&name, EvalNativeUserConstant::Scalar(value)))
}

/// Validates and stores one array-valued user constant after the ABI panic boundary.
///
/// # Safety
/// Mirrors the exported registration function's pointer requirements.
unsafe fn register_native_user_constant_array_inner(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    spec_ptr: *const u8,
    spec_len: u64,
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
    let Some(NativeCallableDefault::Array(elements)) =
        native_callable_array_default(spec_ptr, spec_len)
    else {
        return 0;
    };
    if !user_constant_elements_are_supported(&elements) {
        return 0;
    }
    i32::from(context.define_native_user_constant(&name, EvalNativeUserConstant::Array(elements)))
}

/// Returns whether every decoded element of a user array constant is materializable.
///
/// Walks nested arrays so an object hidden at any depth is rejected before the constant is
/// stored, keeping the registry to values `eval_native_user_constant` can always produce.
fn user_constant_elements_are_supported(elements: &[NativeCallableArrayDefaultElement]) -> bool {
    elements
        .iter()
        .all(|element| user_constant_value_is_supported(&element.value))
}

/// Returns whether one decoded element value is a supported user constant value.
fn user_constant_value_is_supported(value: &NativeCallableDefault) -> bool {
    match value {
        NativeCallableDefault::Null
        | NativeCallableDefault::Bool(_)
        | NativeCallableDefault::Int(_)
        | NativeCallableDefault::Float(_)
        | NativeCallableDefault::String(_)
        | NativeCallableDefault::EmptyArray => true,
        NativeCallableDefault::Array(elements) => user_constant_elements_are_supported(elements),
        NativeCallableDefault::Object { .. } => false,
    }
}

/// Decodes one tagged scalar constant payload shared by both registration families.
///
/// # Safety
/// When `kind` is the string tag, `value_word` must be a pointer readable for
/// `value_len` bytes; every other tag ignores both.
unsafe fn native_scalar_constant_value(
    kind: u64,
    value_word: u64,
    value_len: u64,
) -> Option<EvalNativeGlobalConstant> {
    Some(match kind {
        NATIVE_CONSTANT_NULL => EvalNativeGlobalConstant::Null,
        NATIVE_CONSTANT_BOOL => EvalNativeGlobalConstant::Bool(value_word != 0),
        NATIVE_CONSTANT_INT => EvalNativeGlobalConstant::Int(value_word as i64),
        NATIVE_CONSTANT_FLOAT => EvalNativeGlobalConstant::Float(f64::from_bits(value_word)),
        NATIVE_CONSTANT_STRING => {
            let value = abi_name_to_string(value_word as *const u8, value_len).ok()?;
            EvalNativeGlobalConstant::String(value)
        }
        NATIVE_CONSTANT_RESOURCE => EvalNativeGlobalConstant::Resource(value_word as i64),
        _ => return None,
    })
}
