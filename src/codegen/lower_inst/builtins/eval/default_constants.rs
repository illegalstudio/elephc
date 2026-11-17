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
) -> Option<EvalNativePropertyDefault> {
    if let Some(default) = default {
        if let Some(message) = crate::types::deferred_defaults::unconditional_error_message(default) {
            return Some(EvalNativePropertyDefault::DeferredError(message.to_string()));
        }
        if let Some(value) =
            crate::codegen::const_default_values::resolve_enum_case_default(default,
                &crate::codegen::const_default_values::ConstDefaultContext {
                    module: default_context.module, current_class: default_context.current_class,
                })
        {
            return Some(EvalNativePropertyDefault::EnumCase(value));
        }
        return eval_native_literal_default(default)
            .or_else(|| eval_native_array_default(default, default_context, 0))
            .map(EvalNativePropertyDefault::Value);
    }
    (!is_declared && !is_abstract).then_some(EvalNativeCallableDefault::Scalar {
        kind: NATIVE_DEFAULT_NULL,
        payload: 0,
    }).map(EvalNativePropertyDefault::Value)
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
            encode_eval_native_literal_default_string(bytes, value);
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
        EvalNativeCallableDefault::EnumCase { .. } | EvalNativeCallableDefault::Evaluated { .. } => {
            unreachable!("enum singletons are excluded from compact callable default metadata")
        }
        EvalNativeCallableDefault::Scalar { kind, payload } => {
            bytes.push(NATIVE_OBJECT_DEFAULT_ARG_SCALAR);
            bytes.extend_from_slice(&(*kind as u64).to_le_bytes());
            bytes.extend_from_slice(&(*payload as u64).to_le_bytes());
        }
        EvalNativeCallableDefault::String(value) => {
            bytes.push(NATIVE_OBJECT_DEFAULT_ARG_STRING);
            encode_eval_native_literal_default_string(bytes, value);
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

/// Encodes PHP literal bytes while ordinary class and argument names retain their UTF-8 codec.
fn encode_eval_native_literal_default_string(bytes: &mut Vec<u8>, value: &str) {
    let value = elephc_builtin_contract::string_literal::literal_bytes(value);
    let len = u32::try_from(value.len()).unwrap_or(u32::MAX);
    bytes.extend_from_slice(&len.to_le_bytes());
    bytes.extend_from_slice(&value);
}

/// Looks up a class constant expression, including inherited parent classes.
pub(super) fn eval_native_class_constant_expr<'a>(
    module: &'a Module,
    class_name: &str,
    constant_name: &str,
) -> Option<(&'a str, &'a Expr)> {
    let (resolved_name, class_info) = resolve_eval_native_default_class(module, class_name)?;
    if let Some(value) = class_info.constants.get(constant_name) {
        return Some((resolved_name, value));
    }
    for interface_name in &class_info.interfaces {
        if let Some(value) =
            eval_native_interface_constant_expr(module, interface_name, constant_name)
        {
            return Some(value);
        }
    }
    if let Some(parent_name) = class_info.parent.as_deref() {
        return eval_native_class_constant_expr(module, parent_name, constant_name);
    }
    None
}

/// Looks up an interface constant expression, including inherited interfaces.
pub(super) fn eval_native_interface_constant_expr<'a>(
    module: &'a Module,
    interface_name: &str,
    constant_name: &str,
) -> Option<(&'a str, &'a Expr)> {
    let mut visited = crate::fast_hash::FastSet::default();
    let mut queue = vec![interface_name.to_string()];
    while let Some(name) = queue.pop() {
        let Some((resolved_name, interface_info)) =
            resolve_eval_native_default_interface(module, &name)
        else {
            continue;
        };
        if !visited.insert(php_symbol_key(resolved_name.trim_start_matches('\\'))) {
            continue;
        }
        if let Some(value) = interface_info.constants.get(constant_name) {
            return Some((resolved_name, value));
        }
        queue.extend(interface_info.parents.iter().cloned());
    }
    None
}

/// Looks up class metadata by PHP-style case-insensitive name.
pub(super) fn resolve_eval_native_default_class<'a>(
    module: &'a Module,
    class_name: &str,
) -> Option<(&'a str, &'a ClassInfo)> {
    resolve_case_insensitive_entry(&module.class_infos, class_name)
}

/// Looks up interface metadata by PHP-style case-insensitive name.
pub(super) fn resolve_eval_native_default_interface<'a>(
    module: &'a Module,
    interface_name: &str,
) -> Option<(&'a str, &'a InterfaceInfo)> {
    resolve_case_insensitive_entry(&module.interface_infos, interface_name)
}

#[cfg(test)]
thread_local! {
    /// Counts the linear fallback scans `resolve_case_insensitive_entry` had to make.
    static CASE_INSENSITIVE_SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Finds `name` in a declaration table keyed by the declared spelling, ignoring ASCII case.
///
/// The spelling a reference carries is almost always the declared one, so the exact key is
/// probed first. Only a differently cased reference falls back to a walk of the table, which
/// compares in place instead of lowercasing every candidate into a fresh string: the registration
/// collector asks once per class, constant and ancestor level, and that used to be quadratic in
/// the class count with an allocation per step.
fn resolve_case_insensitive_entry<'a, V, S: std::hash::BuildHasher>(
    table: &'a std::collections::HashMap<String, V, S>,
    name: &str,
) -> Option<(&'a str, &'a V)> {
    let wanted = name.trim_start_matches('\\');
    if let Some((key, value)) = table.get_key_value(wanted) {
        return Some((key.as_str(), value));
    }
    #[cfg(test)]
    CASE_INSENSITIVE_SCANS.with(|scans| scans.set(scans.get() + 1));
    table
        .iter()
        .find(|(candidate, _)| candidate.trim_start_matches('\\').eq_ignore_ascii_case(wanted))
        .map(|(key, value)| (key.as_str(), value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Counts the fallback scans made while `lookup` runs.
    fn scans_during(lookup: impl FnOnce()) -> usize {
        let before = CASE_INSENSITIVE_SCANS.with(|scans| scans.get());
        lookup();
        CASE_INSENSITIVE_SCANS.with(|scans| scans.get()) - before
    }

    /// Verifies a lookup by the declared spelling never walks the table, so a registration pass
    /// over N classes does N probes rather than N * N comparisons.
    #[test]
    fn exact_spelling_lookups_never_scan_the_table() {
        let table: HashMap<String, usize> =
            (0..2_000).map(|index| (format!("App\\Class{index}"), index)).collect();
        let scans = scans_during(|| {
            for index in 0..2_000 {
                let (key, value) =
                    resolve_case_insensitive_entry(&table, &format!("\\App\\Class{index}"))
                        .expect("declared class");
                assert_eq!((key, *value), (format!("App\\Class{index}").as_str(), index));
            }
        });
        assert_eq!(scans, 0);
    }

    /// Verifies a differently cased reference still resolves, through exactly one scan.
    #[test]
    fn differently_cased_lookups_scan_once_and_still_resolve() {
        let table: HashMap<String, usize> =
            (0..50).map(|index| (format!("App\\Class{index}"), index)).collect();
        let mut found = None;
        let scans = scans_during(|| found = resolve_case_insensitive_entry(&table, "app\\CLASS7"));
        assert_eq!(found.map(|(key, value)| (key, *value)), Some(("App\\Class7", 7)));
        assert_eq!(scans, 1);
        assert!(resolve_case_insensitive_entry(&table, "App\\Missing").is_none());
    }
}
