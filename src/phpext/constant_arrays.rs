//! Purpose:
//! Resolves supported constant array and string offsets without native execution.
//!
//! Called from:
//! - The producer constant resolver after lexical references have been expanded.
//!
//! Key details:
//! - Array keys and append positions use the existing compiler normalization rules.
//! - Unpacking reindexes numeric keys and retains source iteration order after overwrites.
//! - Operations that need runtime diagnostics remain unresolved.

use crate::parser::ast::{ArrayEntry, Expr, ExprKind};
use super::constant_values::{array_key, from_expression, ConstantKey};

/// Retains canonical keys alongside their source expressions and resolved values.
type Pair = (ConstantKey, Expr, Expr);

/// Selects an existing static offset while preserving binary strings and PHP key semantics.
pub(super) fn access(array: &Expr, index: &Expr) -> Option<ExprKind> {
    if let ExprKind::StringLiteral(value) = &array.kind {
        let ExprKind::IntLiteral(index) = index.kind else { return None; };
        let bytes = crate::string_bytes::literal_bytes(value);
        let index = if index < 0 { i64::try_from(bytes.len()).ok()?.checked_add(index)? } else { index };
        let byte = *bytes.get(usize::try_from(index).ok()?)?;
        let mut value = String::new();
        crate::string_bytes::push_escaped_byte(byte, &mut value);
        return Some(ExprKind::StringLiteral(value));
    }
    from_expression(array, 0).ok()?;
    let key = array_key(index).ok()?;
    pairs(array, 0)?.into_iter().find_map(|(candidate, _, value)| (candidate == key).then_some(value.kind))
}

/// Expands known unpacking before applying the shared append and duplicate-key rules.
fn pairs(array: &Expr, depth: usize) -> Option<Vec<Pair>> {
    if depth > 64 { return None; }
    let entries = match &array.kind {
        ExprKind::ArrayLiteral(values) => values.iter().map(|value| match &value.kind {
            ExprKind::Spread(inner) => ArrayEntry::Spread((**inner).clone()),
            _ => ArrayEntry::Value(value.clone()),
        }).collect(),
        ExprKind::ArrayLiteralAssoc(values) => values.iter()
            .map(|(key, value)| ArrayEntry::Keyed(key.clone(), value.clone())).collect(),
        ExprKind::ArrayLiteralMixed(entries) => entries.clone(),
        _ => return None,
    };
    let mut flat = Vec::new();
    for entry in entries {
        if let ArrayEntry::Spread(value) = entry {
            let value: &Expr = match &value.kind { ExprKind::Spread(inner) => inner, _ => &value };
            for (key, expression, value) in pairs(value, depth + 1)? {
                flat.push(match key {
                    ConstantKey::Int(_) => ArrayEntry::Value(value),
                    ConstantKey::String(_) => ArrayEntry::Keyed(expression, value),
                });
            }
        } else { flat.push(entry); }
    }
    let ExprKind::ArrayLiteralAssoc(values) = crate::types::fold_spreadless_mixed_array_literal(
        &ExprKind::ArrayLiteralMixed(flat), crate::codegen_support::compile_php_version(),
    )? else { return None; };
    let mut unique: Vec<Pair> = Vec::new();
    for (expression, value) in values {
        let key = array_key(&expression).ok()?;
        if let Some(existing) = unique.iter_mut().find(|(candidate, _, _)| candidate == &key) {
            existing.2 = value;
        } else { unique.push((key, expression, value)); }
    }
    Some(unique)
}
