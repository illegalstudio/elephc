//! Purpose:
//! Resolves constant-backed defaults and encodes compound default values.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Class ancestry, array-key normalization, and binary formats are unchanged.
//! - Class ancestry and key normalization are resolved by the SHARED folder in
//!   `crate::codegen::const_default_values`; only the libelephc-magician binary encoding and the
//!   eval-specific property-default subset live here.

use super::*;

use crate::codegen::const_default_values::const_default_string_array_key;

/// Normalizes one string default-array key to PHP's integer-key rules.
pub(super) fn eval_native_string_array_default_key(
    value: &str,
) -> Option<EvalNativeCallableArrayDefaultKey> {
    const_default_string_array_key(value)
}

/// Converts supported property defaults into the compact eval bridge default ABI.
///
/// Property defaults deliberately stay on the LITERAL and array subset: a property initializer is
/// materialized by object allocation, not by the callable default path, so it must not silently
/// gain object construction.
pub(super) fn eval_native_property_default(
    default: Option<&Expr>,
    is_declared: bool,
    is_abstract: bool,
    default_context: &EvalNativeDefaultContext<'_>,
) -> Option<EvalNativeCallableDefault> {
    if let Some(default) = default {
        return eval_native_literal_default(default)
            .or_else(|| eval_native_array_default(default, default_context, 0));
    }
    (!is_declared && !is_abstract).then_some(EvalNativeCallableDefault::Scalar {
        kind: NATIVE_DEFAULT_NULL,
        payload: 0,
    })
}

/// Encodes an object-valued native callable default for libelephc-magician.
pub(super) fn encode_eval_native_object_default(default: &EvalNativeCallableDefault) -> Vec<u8> {
    let EvalNativeCallableDefault::Object { class_name, args } = default else {
        return Vec::new();
    };
    let mut bytes = Vec::new();
    encode_eval_native_default_string(&mut bytes, class_name);
    bytes.push(args.len() as u8);
    for arg in args {
        encode_eval_native_object_default_arg(&mut bytes, arg);
    }
    bytes
}

/// Encodes an array-valued native callable default for libelephc-magician.
pub(super) fn encode_eval_native_array_default(default: &EvalNativeCallableDefault) -> Vec<u8> {
    let EvalNativeCallableDefault::Array(elements) = default else {
        return Vec::new();
    };
    encode_eval_native_array_default_elements(elements)
}

/// Encodes one array element list into the shared native array-default binary spec.
///
/// The spec is a little-endian `u32` element count followed by one encoded element each, so
/// libelephc-magician's decoder rejects any truncated or trailing input.
pub(super) fn encode_eval_native_array_default_elements(
    elements: &[EvalNativeCallableArrayDefaultElement],
) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(elements.len() as u32).to_le_bytes());
    for element in elements {
        encode_eval_native_array_default_element(&mut bytes, element);
    }
    bytes
}

/// Encodes one array-default element and its optional static key.
pub(super) fn encode_eval_native_array_default_element(
    bytes: &mut Vec<u8>,
    element: &EvalNativeCallableArrayDefaultElement,
) {
    match &element.key {
        Some(EvalNativeCallableArrayDefaultKey::Int(value)) => {
            bytes.push(NATIVE_ARRAY_DEFAULT_KEY_INT);
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        Some(EvalNativeCallableArrayDefaultKey::String(value)) => {
            bytes.push(NATIVE_ARRAY_DEFAULT_KEY_STRING);
            encode_eval_native_default_string(bytes, value);
        }
        None => bytes.push(NATIVE_ARRAY_DEFAULT_KEY_AUTO),
    }
    encode_eval_native_object_default_arg_value(bytes, &element.default);
}

/// Encodes one object-default constructor argument for libelephc-magician.
pub(super) fn encode_eval_native_object_default_arg(
    bytes: &mut Vec<u8>,
    arg: &EvalNativeCallableObjectDefaultArg,
) {
    if let Some(name) = &arg.name {
        bytes.push(NATIVE_OBJECT_DEFAULT_ARG_NAMED);
        encode_eval_native_default_string(bytes, name);
    }
    encode_eval_native_object_default_arg_value(bytes, &arg.default);
}

/// Encodes one object-default constructor argument value for libelephc-magician.
pub(super) fn encode_eval_native_object_default_arg_value(
    bytes: &mut Vec<u8>,
    default: &EvalNativeCallableDefault,
) {
    match default {
        EvalNativeCallableDefault::Scalar { kind, payload } => {
            bytes.push(NATIVE_OBJECT_DEFAULT_ARG_SCALAR);
            bytes.extend_from_slice(&(*kind as u64).to_le_bytes());
            bytes.extend_from_slice(&(*payload as u64).to_le_bytes());
        }
        EvalNativeCallableDefault::String(value) => {
            bytes.push(NATIVE_OBJECT_DEFAULT_ARG_STRING);
            encode_eval_native_default_string(bytes, value);
        }
        EvalNativeCallableDefault::Object { .. } => {
            bytes.push(NATIVE_OBJECT_DEFAULT_ARG_OBJECT);
            bytes.extend_from_slice(&encode_eval_native_object_default(default));
        }
        EvalNativeCallableDefault::Array(_) => {
            bytes.push(NATIVE_OBJECT_DEFAULT_ARG_ARRAY);
            bytes.extend_from_slice(&encode_eval_native_array_default(default));
        }
    }
}

/// Encodes one UTF-8 string with a little-endian u32 byte-length prefix.
pub(super) fn encode_eval_native_default_string(bytes: &mut Vec<u8>, value: &str) {
    let len = u32::try_from(value.len()).unwrap_or(u32::MAX);
    bytes.extend_from_slice(&len.to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
}
