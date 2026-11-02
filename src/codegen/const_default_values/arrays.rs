//! Purpose:
//! Resolves pure array default trees, including ordered keys, duplicates and unpacking.
//!
//! Called from:
//! - `super::resolve_const_default_at()` and native/eval constant materializers.
//!
//! Key details:
//! - Integer spread keys are renumbered; string keys overwrite at their first position.
//! - Overflow and unsupported values decline without changing lazy error behavior.

use super::*;
use crate::parser::ast::ArrayEntry;

/// Resolves every representable constant array shape through the same ordered entry fold.
pub(in crate::codegen) fn resolve_array_const_default(
    expr: &Expr, context: &ConstDefaultContext<'_>, depth: usize,
) -> Option<ConstDefaultValue> {
    // Keep ordinary defaults in evaluation order, including overwritten object constructors.
    match &expr.kind {
        ExprKind::ArrayLiteral(items) if items.iter().all(|item| !matches!(item.kind, ExprKind::Spread(_))) => {
            return items.iter().map(|value| Some(ConstDefaultArrayElement {
                key: None, default: resolve_const_default_at(value, context, depth + 1)?,
            })).collect::<Option<Vec<_>>>().map(ConstDefaultValue::Array);
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            return items.iter().map(|(key, value)| Some(ConstDefaultArrayElement {
                key: Some(resolve_array_const_default_key(key, context, depth + 1)?),
                default: resolve_const_default_at(value, context, depth + 1)?,
            })).collect::<Option<Vec<_>>>().map(ConstDefaultValue::Array);
        }
        _ => {}
    }
    if let Some(folded) = crate::types::fold_spreadless_mixed_array_literal(
        &expr.kind, crate::codegen_support::compile_php_version(),
    ) {
        return resolve_array_const_default(&Expr::new(folded, expr.span), context, depth);
    }
    let entries = match &expr.kind {
        ExprKind::ArrayLiteral(items) => items.iter().map(|value| {
            if matches!(value.kind, ExprKind::Spread(_)) {
                ArrayEntry::Spread(value.clone())
            } else { ArrayEntry::Value(value.clone()) }
        }).collect::<Vec<_>>(),
        ExprKind::ArrayLiteralAssoc(items) => items.iter()
            .map(|(key, value)| ArrayEntry::Keyed(key.clone(), value.clone())).collect(),
        ExprKind::ArrayLiteralMixed(entries) => entries.clone(),
        _ => return None,
    };
    let mut elements = Vec::with_capacity(entries.len());
    let mut highest = None;
    for entry in entries {
        match entry {
            ArrayEntry::Keyed(key, value) => {
                let key = resolve_array_const_default_key(&key, context, depth + 1)?;
                let value = resolve_const_default_at(&value, context, depth + 1)?;
                if contains_object(&value) { return None; }
                insert_array_element(&mut elements, &mut highest, key, value);
            }
            ArrayEntry::Value(value) => {
                let key = next_integer_key(highest)?;
                let value = resolve_const_default_at(&value, context, depth + 1)?;
                if contains_object(&value) { return None; }
                insert_array_element(&mut elements, &mut highest, ConstDefaultArrayKey::Int(key), value);
            }
            ArrayEntry::Spread(value) => {
                let source = match &value.kind { ExprKind::Spread(inner) => inner.as_ref(), _ => &value };
                let source = match resolve_const_default_at(source, context, depth + 1)? {
                    ConstDefaultValue::Array(elements) => elements,
                    ConstDefaultValue::Scalar { kind: CONST_DEFAULT_EMPTY_ARRAY, .. } => Vec::new(),
                    _ => return None,
                };
                if source.iter().any(|element| contains_object(&element.default)) { return None; }
                // A spread reads the source array after duplicate writes have collapsed.
                let mut source_values = Vec::with_capacity(source.len());
                let mut source_highest = None;
                for (index, element) in source.into_iter().enumerate() {
                    let key = element.key.unwrap_or(ConstDefaultArrayKey::Int(index as i64));
                    insert_array_element(&mut source_values, &mut source_highest, key, element.default);
                }
                for element in source_values {
                    let key = match element.key {
                        Some(ConstDefaultArrayKey::String(key)) => ConstDefaultArrayKey::String(key),
                        _ => ConstDefaultArrayKey::Int(next_integer_key(highest)?),
                    };
                    insert_array_element(&mut elements, &mut highest, key, element.default);
                }
            }
        }
    }
    if elements.iter().enumerate().all(|(index, element)|
        element.key == Some(ConstDefaultArrayKey::Int(index as i64))) {
        for element in &mut elements { element.key = None; }
    }
    Some(ConstDefaultValue::Array(elements))
}

/// Selects the next append key lazily, including PHP's pre-8.3 negative-key rule.
fn next_integer_key(highest: Option<i64>) -> Option<i64> {
    match highest {
        None => Some(0),
        Some(key) if key < 0 && crate::codegen_support::compile_php_version()
            < crate::php_version::PhpVersion::Php83 => Some(0),
        Some(key) => key.checked_add(1),
    }
}

/// Keeps the highest integer cursor and PHP's first-position/last-value duplicate rule.
fn insert_array_element(
    elements: &mut Vec<ConstDefaultArrayElement>, highest: &mut Option<i64>,
    key: ConstDefaultArrayKey, value: ConstDefaultValue,
) {
    if let ConstDefaultArrayKey::Int(key) = &key {
        *highest = Some(highest.map_or(*key, |current| current.max(*key)));
    }
    if let Some(element) = elements.iter_mut().find(|element| match (element.key.as_ref(), &key) {
        (Some(ConstDefaultArrayKey::String(existing)), ConstDefaultArrayKey::String(candidate)) =>
            elephc_builtin_contract::string_literal::literal_bytes(existing)
                == elephc_builtin_contract::string_literal::literal_bytes(candidate),
        (existing, candidate) => existing == Some(candidate),
    }) {
        element.default = value;
    } else {
        elements.push(ConstDefaultArrayElement { key: Some(key), default: value });
    }
}

/// Resolves one supported static array key.
fn resolve_array_const_default_key(
    expr: &Expr,
    context: &ConstDefaultContext<'_>,
    depth: usize,
) -> Option<ConstDefaultArrayKey> {
    if let Some(key) = resolve_literal_array_const_default_key(expr) {
        return Some(key);
    }
    match resolve_const_default_at(expr, context, depth + 1)? {
        ConstDefaultValue::Scalar {
            kind: CONST_DEFAULT_NULL,
            ..
        } => Some(ConstDefaultArrayKey::String(String::new())),
        ConstDefaultValue::Scalar {
            kind: CONST_DEFAULT_BOOL,
            payload,
        } => Some(ConstDefaultArrayKey::Int((payload != 0) as i64)),
        ConstDefaultValue::Scalar {
            kind: CONST_DEFAULT_INT,
            payload,
        } => Some(ConstDefaultArrayKey::Int(payload)),
        ConstDefaultValue::Scalar {
            kind: CONST_DEFAULT_FLOAT,
            payload,
        } => Some(ConstDefaultArrayKey::Int(
            crate::types::php_float_array_key(f64::from_bits(payload as u64))
        )),
        ConstDefaultValue::String(value) => const_default_string_array_key(&value),
        _ => None,
    }
}

/// Resolves one literal static array key.
pub(in crate::codegen) fn resolve_literal_array_const_default_key(
    expr: &Expr,
) -> Option<ConstDefaultArrayKey> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(ConstDefaultArrayKey::Int(*value)),
        ExprKind::BoolLiteral(value) => Some(ConstDefaultArrayKey::Int(i64::from(*value))),
        ExprKind::FloatLiteral(value) => Some(ConstDefaultArrayKey::Int(crate::types::php_float_array_key(*value))),
        ExprKind::StringLiteral(value) => const_default_string_array_key(value),
        ExprKind::Null => Some(ConstDefaultArrayKey::String(String::new())),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value.checked_neg().map(ConstDefaultArrayKey::Int),
            ExprKind::FloatLiteral(value) => Some(ConstDefaultArrayKey::Int(crate::types::php_float_array_key(-*value))),
            _ => None,
        },
        _ => None,
    }
}


/// Keeps effectful object construction outside the pure unpacking folder.
fn contains_object(value: &ConstDefaultValue) -> bool {
    match value {
        ConstDefaultValue::Object { .. } => true,
        ConstDefaultValue::Array(elements) => elements.iter().any(|element| contains_object(&element.default)),
        _ => false,
    }
}
