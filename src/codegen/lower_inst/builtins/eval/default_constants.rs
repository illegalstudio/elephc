//! Purpose:
//! Resolves constant-backed defaults and encodes compound default values.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Class ancestry, array-key normalization, and binary formats are unchanged.

use super::*;

/// Resolves and materializes one global constant default expression.
pub(super) fn eval_native_global_constant_default(
    default_context: &EvalNativeDefaultContext<'_>,
    name: &str,
    depth: usize,
) -> Option<EvalNativeCallableDefault> {
    let expr_kind = default_context
        .module
        .global_constants
        .get(name)
        .or_else(|| {
            default_context
                .module
                .global_constants
                .get(name.trim_start_matches('\\'))
        })
        .map(|(expr_kind, _)| expr_kind.clone())?;
    let expr = Expr::new(expr_kind, crate::span::Span::dummy());
    eval_native_callable_default_at(&expr, default_context, depth + 1)
}

/// Resolves and materializes one class-like constant default expression.
pub(super) fn eval_native_scoped_constant_default(
    default_context: &EvalNativeDefaultContext<'_>,
    receiver: &StaticReceiver,
    constant_name: &str,
    depth: usize,
) -> Option<EvalNativeCallableDefault> {
    let class_name = eval_native_static_receiver_name(default_context, receiver)?;
    if let Some((declaring_name, value)) =
        eval_native_class_constant_expr(default_context.module, &class_name, constant_name)
    {
        let nested_context =
            EvalNativeDefaultContext::for_class(default_context.module, declaring_name);
        return eval_native_callable_default_at(value, &nested_context, depth + 1);
    }
    if let Some((declaring_name, value)) =
        eval_native_interface_constant_expr(default_context.module, &class_name, constant_name)
    {
        let nested_context =
            EvalNativeDefaultContext::for_class(default_context.module, declaring_name);
        return eval_native_callable_default_at(value, &nested_context, depth + 1);
    }
    if let Some((declaring_name, value)) =
        eval_native_trait_constant_expr(default_context.module, &class_name, constant_name)
    {
        let nested_context =
            EvalNativeDefaultContext::for_class(default_context.module, declaring_name);
        return eval_native_callable_default_at(value, &nested_context, depth + 1);
    }
    None
}

/// Resolves `self`, `static`, `parent`, or a named receiver for default constants.
pub(super) fn eval_native_static_receiver_name(
    default_context: &EvalNativeDefaultContext<'_>,
    receiver: &StaticReceiver,
) -> Option<String> {
    match receiver {
        StaticReceiver::Named(name) => {
            Some(name.as_canonical().trim_start_matches('\\').to_string())
        }
        StaticReceiver::Self_ | StaticReceiver::Static => {
            default_context.current_class.map(str::to_string)
        }
        StaticReceiver::Parent => {
            let current = default_context.current_class?;
            resolve_eval_native_default_class(default_context.module, current)
                .and_then(|(_, class_info)| class_info.parent.clone())
        }
    }
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
    let mut visited = std::collections::HashSet::new();
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

/// Looks up a direct trait constant expression by PHP-style trait name.
pub(super) fn eval_native_trait_constant_expr<'a>(
    module: &'a Module,
    trait_name: &str,
    constant_name: &str,
) -> Option<(&'a str, &'a Expr)> {
    let trait_key = php_symbol_key(trait_name.trim_start_matches('\\'));
    let resolved_name = module
        .trait_table
        .names
        .iter()
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == trait_key)?;
    let value = module
        .declared_trait_constants
        .get(resolved_name)
        .and_then(|constants| constants.get(constant_name))?;
    Some((resolved_name.as_str(), value))
}

/// Looks up class metadata by PHP-style case-insensitive name.
pub(super) fn resolve_eval_native_default_class<'a>(
    module: &'a Module,
    class_name: &str,
) -> Option<(&'a str, &'a ClassInfo)> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    module
        .class_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == class_key)
        .map(|(name, info)| (name.as_str(), info))
}

/// Looks up interface metadata by PHP-style case-insensitive name.
pub(super) fn resolve_eval_native_default_interface<'a>(
    module: &'a Module,
    interface_name: &str,
) -> Option<(&'a str, &'a InterfaceInfo)> {
    let interface_key = php_symbol_key(interface_name.trim_start_matches('\\'));
    module
        .interface_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == interface_key)
        .map(|(name, info)| (name.as_str(), info))
}

/// Converts one literal static array key into bridge metadata.
pub(super) fn eval_native_literal_array_default_key(expr: &Expr) -> Option<EvalNativeCallableArrayDefaultKey> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(EvalNativeCallableArrayDefaultKey::Int(*value)),
        ExprKind::BoolLiteral(value) => {
            Some(EvalNativeCallableArrayDefaultKey::Int(i64::from(*value)))
        }
        ExprKind::FloatLiteral(value) => {
            Some(EvalNativeCallableArrayDefaultKey::Int(*value as i64))
        }
        ExprKind::StringLiteral(value) => eval_native_string_array_default_key(value),
        ExprKind::Null => Some(EvalNativeCallableArrayDefaultKey::String(String::new())),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value
                .checked_neg()
                .map(EvalNativeCallableArrayDefaultKey::Int),
            ExprKind::FloatLiteral(value) => {
                Some(EvalNativeCallableArrayDefaultKey::Int((-*value) as i64))
            }
            _ => None,
        },
        _ => None,
    }
}

/// Normalizes one string default-array key to PHP's integer-key rules.
pub(super) fn eval_native_string_array_default_key(value: &str) -> Option<EvalNativeCallableArrayDefaultKey> {
    if is_php_integer_array_key(value) {
        value
            .parse::<i64>()
            .ok()
            .map(EvalNativeCallableArrayDefaultKey::Int)
    } else {
        Some(EvalNativeCallableArrayDefaultKey::String(value.to_string()))
    }
}

/// Converts supported property defaults into the compact eval bridge default ABI.
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

/// Converts a negated literal default into the compact eval bridge default ABI.
pub(super) fn eval_native_callable_negated_default(expr: &Expr) -> Option<EvalNativeCallableDefault> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => {
            value
                .checked_neg()
                .map(|payload| EvalNativeCallableDefault::Scalar {
                    kind: NATIVE_DEFAULT_INT,
                    payload,
                })
        }
        ExprKind::FloatLiteral(value) => Some(EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_FLOAT,
            payload: (-*value).to_bits() as i64,
        }),
        _ => None,
    }
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
