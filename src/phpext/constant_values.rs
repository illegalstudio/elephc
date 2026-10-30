//! Purpose:
//! Validates resolved constant trees and materializes persistent Zend metadata values.
//!
//! Called from:
//! - Global constant selection and exported class/property registration.
//!
//! Key details:
//! - Zend performs array insertion and unpacking key semantics.
//! - Immutable arrays contain only immutable children and survive until module unload.

use crate::{errors::CompileError, parser::ast::{ArrayEntry, Expr, ExprKind}};

/// One persistent constant tree, independent of the request-owned native heap.
#[derive(Clone, Debug)]
pub enum ConstantValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Vec<u8>),
    Array(Vec<ConstantArrayEntry>),
}

/// Source-order constant entries whose final insertion semantics belong to Zend.
#[derive(Clone, Debug)]
pub enum ConstantArrayEntry {
    Value(ConstantValue),
    Keyed(ConstantKey, ConstantValue),
    Spread(ConstantValue),
}

/// PHP-compatible keys after shared numeric-string and float-key classification.
#[derive(Clone, Debug)]
pub enum ConstantKey { Int(i64), String(Vec<u8>) }

/// Converts a fully resolved expression without evaluating unknown code or following references.
pub(super) fn from_expression(expr: &Expr, depth: usize) -> Result<ConstantValue, CompileError> {
    if depth > 64 { return Err(CompileError::new(expr.span, "PHP extension constant nesting limit exceeded")); }
    let convert = |expr: &Expr| from_expression(expr, depth + 1);
    Ok(match &expr.kind {
        ExprKind::Null => ConstantValue::Null,
        ExprKind::BoolLiteral(value) => ConstantValue::Bool(*value),
        ExprKind::IntLiteral(value) => ConstantValue::Int(*value),
        ExprKind::FloatLiteral(value) => ConstantValue::Float(*value),
        ExprKind::StringLiteral(value) => ConstantValue::String(crate::string_bytes::literal_bytes(value)),
        ExprKind::ArrayLiteral(values) => ConstantValue::Array(values.iter().map(|value| {
            if let ExprKind::Spread(inner) = &value.kind { spread(inner, depth + 1) }
            else { convert(value).map(ConstantArrayEntry::Value) }
        }).collect::<Result<Vec<_>, _>>()?),
        ExprKind::ArrayLiteralAssoc(values) => ConstantValue::Array(values.iter()
            .map(|(key, value)| Ok(ConstantArrayEntry::Keyed(array_key(key)?, convert(value)?)))
            .collect::<Result<Vec<_>, CompileError>>()?),
        ExprKind::ArrayLiteralMixed(values) => ConstantValue::Array(values.iter().map(|entry| Ok(match entry {
            ArrayEntry::Value(value) => ConstantArrayEntry::Value(convert(value)?),
            ArrayEntry::Keyed(key, value) => ConstantArrayEntry::Keyed(array_key(key)?, convert(value)?),
            ArrayEntry::Spread(value) => {
                let value = if let ExprKind::Spread(inner) = &value.kind { inner } else { value };
                spread(value, depth + 1)?
            }
        })).collect::<Result<Vec<_>, CompileError>>()?),
        _ => return Err(CompileError::new(expr.span, "PHP extension metadata requires a statically resolved scalar or array constant")),
    })
}

/// Admits constant array unpacking after removing the parser's retained spread expression wrapper.
fn spread(expr: &Expr, depth: usize) -> Result<ConstantArrayEntry, CompileError> {
    let value = from_expression(expr, depth)?;
    if !matches!(value, ConstantValue::Array(_)) {
        return Err(CompileError::new(expr.span, "PHP extension constant unpacking requires a constant array"));
    }
    Ok(ConstantArrayEntry::Spread(value))
}

/// Applies the shared compiler's PHP array-key rules without inventing another key parser.
fn array_key(expr: &Expr) -> Result<ConstantKey, CompileError> {
    Ok(match &expr.kind {
        ExprKind::IntLiteral(value) => ConstantKey::Int(*value),
        ExprKind::BoolLiteral(value) => ConstantKey::Int(i64::from(*value)),
        ExprKind::FloatLiteral(value) if value.is_finite() && value.fract() == 0.0 =>
            ConstantKey::Int(crate::types::php_float_array_key(*value)),
        ExprKind::Null => ConstantKey::String(Vec::new()),
        ExprKind::StringLiteral(value) if crate::types::is_php_integer_array_key(value) =>
            ConstantKey::Int(value.parse().map_err(|_| CompileError::new(expr.span, "invalid constant integer array key"))?),
        ExprKind::StringLiteral(value) => ConstantKey::String(crate::string_bytes::literal_bytes(value)),
        _ => return Err(CompileError::new(expr.span, "PHP extension constants require lossless scalar array keys")),
    })
}

/// Emits one persistent metadata value using interned strings and module-owned immutable arrays.
pub(super) fn persistent_value(value: &ConstantValue, destination: &str) -> String {
    emit_value(value, destination, &mut 0)
}

/// Emits nested array values in distinct C scopes and leaves no request-owned allocation behind.
fn emit_value(value: &ConstantValue, destination: &str, next: &mut usize) -> String {
    use super::producer::c_string;
    match value {
        ConstantValue::Null => format!("ZVAL_NULL({destination});"),
        ConstantValue::Bool(value) => format!("ZVAL_BOOL({destination}, {});", u8::from(*value)),
        ConstantValue::Int(value) => format!("ZVAL_LONG({destination}, (zend_long)UINT64_C({}));", *value as u64),
        ConstantValue::Float(value) => format!("ZVAL_DOUBLE({destination}, {});", float_literal(*value)),
        ConstantValue::String(value) => format!("ZVAL_STR({destination}, zend_string_init_interned({}, {}, true));", c_string(value), value.len()),
        ConstantValue::Array(entries) => {
            let id = *next;
            *next += 1;
            let array = format!("ep_constant_array_{id}");
            let mut text = format!("HashTable *{array} = ep_persistent_array_new({});\n", entries.len());
            for entry in entries {
                let value = match entry { ConstantArrayEntry::Value(value) | ConstantArrayEntry::Keyed(_, value)
                    | ConstantArrayEntry::Spread(value) => value };
                text.push_str("{ zval child; ");
                text.push_str(&emit_value(value, "&child", next));
                match entry {
                    ConstantArrayEntry::Value(_) => text.push_str(&format!("\nif (!zend_hash_next_index_insert({array}, &child)) {{ return FAILURE; }}")),
                    ConstantArrayEntry::Keyed(ConstantKey::Int(key), _) => text.push_str(&format!("\nzend_hash_index_update({array}, (zend_ulong)UINT64_C({}), &child);", *key as u64)),
                    ConstantArrayEntry::Keyed(ConstantKey::String(key), _) => text.push_str(&format!("\nzend_hash_update({array}, zend_string_init_interned({}, {}, true), &child);", c_string(key), key.len())),
                    ConstantArrayEntry::Spread(_) => text.push_str(&format!("\nzend_ulong index; zend_string *key; zval *value;\nZEND_HASH_FOREACH_KEY_VAL(Z_ARRVAL(child), index, key, value) {{\n    (void)index; if (key) {{ zend_hash_update({array}, key, value); }}\n    else if (!zend_hash_next_index_insert({array}, value)) {{ return FAILURE; }}\n}} ZEND_HASH_FOREACH_END();")),
                }
                text.push_str("\n}\n");
            }
            text.push_str(&format!("GC_SET_REFCOUNT({array}, 2); GC_ADD_FLAGS({array}, IS_ARRAY_IMMUTABLE); HT_FLAGS({array}) |= HASH_FLAG_STATIC_KEYS;\nZVAL_ARR({destination}, {array}); Z_TYPE_INFO_P({destination}) = IS_ARRAY;"));
            text
        }
    }
}

/// Keeps signed zero and non-finite scalar values valid in SDK C source.
pub(super) fn float_literal(value: f64) -> String {
    if value.is_nan() { "__builtin_nan(\"\")".into() }
    else if value == f64::INFINITY { "__builtin_inf()".into() }
    else if value == f64::NEG_INFINITY { "-__builtin_inf()".into() }
    else { format!("{value:?}") }
}

/// Includes persistent array storage only when generated metadata actually uses arrays.
pub(super) fn support() -> &'static str { include_str!("constant_values.c") }
