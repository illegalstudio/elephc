//! Purpose:
//! Decodes generated callable type and default-value ABI metadata and splits
//! class-member registration keys.
//!
//! Called from:
//! - Native method and constructor registration helpers in this module tree.
//! - `crate::ffi::native_functions` for shared callable metadata decoding.
//!
//! Key details:
//! - Nested object and array defaults are decoded recursively with strict bounds.

use super::*;

/// Decodes tagged default kind/payload ABI fields into native callable metadata.
pub(in crate::ffi) fn native_callable_scalar_default(
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
        NATIVE_DEFAULT_EMPTY_ARRAY => Some(NativeCallableDefault::EmptyArray),
        _ => None,
    }
}

/// Decodes an object-valued native callable default from a generated binary spec.
///
/// # Safety
/// `spec_ptr` must be readable for `spec_len` bytes when non-null.
pub(in crate::ffi) unsafe fn native_callable_object_default(
    spec_ptr: *const u8,
    spec_len: u64,
) -> Option<NativeCallableDefault> {
    let len = usize::try_from(spec_len).ok()?;
    let bytes = (!spec_ptr.is_null()).then(|| std::slice::from_raw_parts(spec_ptr, len))?;
    let mut offset = 0;
    let default = native_callable_object_default_from_bytes(bytes, &mut offset)?;
    (offset == bytes.len()).then_some(default)
}

/// Decodes an array-valued native callable default from a generated binary spec.
///
/// # Safety
/// `spec_ptr` must be readable for `spec_len` bytes when non-null.
pub(in crate::ffi) unsafe fn native_callable_array_default(
    spec_ptr: *const u8,
    spec_len: u64,
) -> Option<NativeCallableDefault> {
    let len = usize::try_from(spec_len).ok()?;
    let bytes = (!spec_ptr.is_null()).then(|| std::slice::from_raw_parts(spec_ptr, len))?;
    let mut offset = 0;
    let default = native_callable_array_default_from_bytes(bytes, &mut offset)?;
    (offset == bytes.len()).then_some(default)
}

/// Decodes an object-valued native callable default from a generated binary spec slice.
fn native_callable_object_default_from_bytes(
    bytes: &[u8],
    offset: &mut usize,
) -> Option<NativeCallableDefault> {
    let class_name = native_attribute_take_string(bytes, offset)?;
    let arg_count = usize::from(native_attribute_take_u8(bytes, offset)?);
    if arg_count > MAX_NATIVE_OBJECT_DEFAULT_ARGS {
        return None;
    }
    let mut args = Vec::with_capacity(arg_count);
    for _ in 0..arg_count {
        args.push(native_callable_object_default_arg(bytes, offset)?);
    }
    Some(NativeCallableDefault::Object { class_name, args })
}

/// Decodes an array-valued native callable default from a generated binary spec slice.
fn native_callable_array_default_from_bytes(
    bytes: &[u8],
    offset: &mut usize,
) -> Option<NativeCallableDefault> {
    let len = usize::try_from(native_attribute_take_u32(bytes, offset)?).ok()?;
    let mut elements = Vec::with_capacity(len);
    for _ in 0..len {
        elements.push(native_callable_array_default_element(bytes, offset)?);
    }
    Some(NativeCallableDefault::Array(elements))
}

/// Decodes one array-default element and its optional static key.
fn native_callable_array_default_element(
    bytes: &[u8],
    offset: &mut usize,
) -> Option<NativeCallableArrayDefaultElement> {
    let key = match native_attribute_take_u8(bytes, offset)? {
        NATIVE_ARRAY_DEFAULT_KEY_AUTO => None,
        NATIVE_ARRAY_DEFAULT_KEY_INT => {
            Some(NativeCallableArrayDefaultKey::Int(native_attribute_take_i64(
                bytes, offset,
            )?))
        }
        NATIVE_ARRAY_DEFAULT_KEY_STRING => Some(NativeCallableArrayDefaultKey::String(
            native_attribute_take_string(bytes, offset)?,
        )),
        _ => return None,
    };
    let value = native_callable_object_default_arg_value(bytes, offset)?;
    Some(NativeCallableArrayDefaultElement { key, value })
}

/// Decodes one object-default constructor argument from a generated binary spec.
fn native_callable_object_default_arg(
    bytes: &[u8],
    offset: &mut usize,
) -> Option<NativeCallableObjectDefaultArg> {
    let tag = native_attribute_take_u8(bytes, offset)?;
    if tag == NATIVE_OBJECT_DEFAULT_ARG_NAMED {
        let name = native_attribute_take_string(bytes, offset)?;
        let value = native_callable_object_default_arg_value(bytes, offset)?;
        return Some(NativeCallableObjectDefaultArg::named(name, value));
    }
    native_callable_object_default_arg_value_for_tag(tag, bytes, offset)
        .map(NativeCallableObjectDefaultArg::positional)
}

/// Decodes one object-default constructor argument value from a generated binary spec.
fn native_callable_object_default_arg_value(
    bytes: &[u8],
    offset: &mut usize,
) -> Option<NativeCallableDefault> {
    let tag = native_attribute_take_u8(bytes, offset)?;
    native_callable_object_default_arg_value_for_tag(tag, bytes, offset)
}

/// Decodes one tagged object-default constructor argument value.
fn native_callable_object_default_arg_value_for_tag(
    tag: u8,
    bytes: &[u8],
    offset: &mut usize,
) -> Option<NativeCallableDefault> {
    match tag {
        NATIVE_OBJECT_DEFAULT_ARG_SCALAR => {
            let kind = native_attribute_take_u64(bytes, offset)?;
            let payload = native_attribute_take_u64(bytes, offset)?;
            native_callable_scalar_default(kind, payload)
        }
        NATIVE_OBJECT_DEFAULT_ARG_STRING => {
            native_attribute_take_string(bytes, offset).map(NativeCallableDefault::String)
        }
        NATIVE_OBJECT_DEFAULT_ARG_OBJECT => native_callable_object_default_from_bytes(bytes, offset),
        NATIVE_OBJECT_DEFAULT_ARG_ARRAY => native_callable_array_default_from_bytes(bytes, offset),
        _ => None,
    }
}

/// Reads one little-endian u64 from a native binary metadata record.
pub(super) fn native_attribute_take_u64(bytes: &[u8], offset: &mut usize) -> Option<u64> {
    let chunk = native_attribute_take_bytes(bytes, offset, std::mem::size_of::<u64>())?;
    Some(u64::from_le_bytes(chunk.try_into().ok()?))
}

/// Decodes one generated type-spec string into eval Reflection type metadata.
pub(in crate::ffi) fn native_callable_type_from_abi(
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
pub(super) fn split_method_key(method_key: &str) -> Option<(&str, &str)> {
    let (class_name, method_name) = method_key.rsplit_once("::")?;
    (!class_name.is_empty() && !method_name.is_empty()).then_some((class_name, method_name))
}

/// Splits one generated `ClassName::propertyName` metadata key into class and property pieces.
pub(super) fn split_property_key(property_key: &str) -> Option<(&str, &str)> {
    split_method_key(property_key)
}

/// Splits `ClassLike::DeclaringClassLike::propertyName` property metadata keys.
pub(super) fn split_three_part_property_key(
    property_key: &str,
) -> Option<(&str, &str, &str)> {
    let (owner_key, property_name) = property_key.rsplit_once("::")?;
    let (class_like_name, declaring_class_like_name) = owner_key.rsplit_once("::")?;
    (!class_like_name.is_empty()
        && !declaring_class_like_name.is_empty()
        && !property_name.is_empty())
    .then_some((class_like_name, declaring_class_like_name, property_name))
}
