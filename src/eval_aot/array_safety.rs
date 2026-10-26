//! Purpose:
//! Checks static array sources, keys, values, and auto-key behavior.
//!
//! Called from:
//! - The eval AOT facade and sibling analysis modules.
//!
//! Key details:
//! - PHP integer-key normalization remains compile-time safe.

use super::*;

/// Returns true when an array source can be materialized inside eval EIR AOT.
pub(super) fn expr_is_eir_static_array_source_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::ArrayLiteral(_)
        | ExprKind::ArrayLiteralAssoc(_)
        | ExprKind::ArrayLiteralMixed(_) => {
            expr_is_eir_static_array_literal_source_safe(expr, support, facts, scope_reads)
        }
        ExprKind::Variable(name) => facts.is_array_local(name),
        _ => false,
    }
}

/// Returns true when a literal array expression can be materialized inside eval EIR AOT.
pub(super) fn expr_is_eir_static_array_literal_source_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => items
            .iter()
            .all(|item| expr_is_eir_static_array_value_safe(item, support, facts, scope_reads)),
        ExprKind::ArrayLiteralAssoc(pairs) => {
            expr_is_eir_static_assoc_array_source_safe(pairs, support, facts, scope_reads)
        }
        ExprKind::ArrayLiteralMixed(entries) => entries.iter().all(|entry| match entry {
            ArrayEntry::Spread(_) => false,
            ArrayEntry::Keyed(key, value) => {
                expr_is_eir_static_array_key_safe(key, support, facts, scope_reads)
                    && expr_is_eir_constant_array_value_safe(value)
            }
            ArrayEntry::Value(value) => expr_is_eir_constant_array_value_safe(value),
        }),
        _ => false,
    }
}

/// Returns true when a mixed-literal value is constant and has no scope or runtime effects.
///
/// Mixed literals retain implicit keys until lowering applies the selected PHP profile, so the
/// eval AOT path accepts only values whose evaluation does not need scope-read or effect analysis.
pub(super) fn expr_is_eir_constant_array_value_safe(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => true,
        ExprKind::Negate(inner) | ExprKind::Not(inner) | ExprKind::BitNot(inner) => {
            expr_is_eir_constant_array_value_safe(inner)
        }
        ExprKind::ArrayLiteral(items) => items.iter().all(expr_is_eir_constant_array_value_safe),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs.iter().all(|(key, value)| {
            expr_is_eir_constant_array_key(key) && expr_is_eir_constant_array_value_safe(value)
        }),
        ExprKind::ArrayLiteralMixed(entries) => entries.iter().all(|entry| match entry {
            ArrayEntry::Spread(_) => false,
            ArrayEntry::Keyed(key, value) => {
                expr_is_eir_constant_array_key(key)
                    && expr_is_eir_constant_array_value_safe(value)
            }
            ArrayEntry::Value(value) => expr_is_eir_constant_array_value_safe(value),
        }),
        _ => false,
    }
}

/// Returns true for constant keys that need no caller scope or effect analysis.
fn expr_is_eir_constant_array_key(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::IntLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::Null => true,
        ExprKind::FloatLiteral(_) => static_integral_float_array_key_value(expr).is_some(),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(_) => static_integer_array_key_value(expr).is_some(),
            ExprKind::FloatLiteral(_) => static_integral_float_array_key_value(expr).is_some(),
            _ => false,
        },
        _ => false,
    }
}

/// Returns true when an associative array source has statically reconstructable key semantics.
pub(super) fn expr_is_eir_static_assoc_array_source_safe<S>(
    pairs: &[(Expr, Expr)],
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    let mut next_auto_key = 0i64;
    let mut auto_key_initialized = false;
    for (key, value) in pairs {
        if static_assoc_key_is_parser_generated(key, value) {
            let ExprKind::IntLiteral(generated) = &key.kind else {
                return false;
            };
            if *generated != next_auto_key {
                return false;
            }
            if !expr_is_eir_static_array_value_safe(value, support, facts, scope_reads) {
                return false;
            }
            advance_static_array_auto_key(&mut next_auto_key, &mut auto_key_initialized);
            continue;
        }
        if !expr_is_eir_static_array_key_safe(key, support, facts, scope_reads)
            || !expr_is_eir_static_array_value_safe(value, support, facts, scope_reads)
        {
            return false;
        }
        update_static_array_auto_key_from_explicit_key(
            key,
            &mut next_auto_key,
            &mut auto_key_initialized,
        );
    }
    true
}

/// Returns true when a static array key can be lowered without eval bridge state.
pub(super) fn expr_is_eir_static_array_key_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::IntLiteral(_) | ExprKind::BoolLiteral(_) | ExprKind::Null => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        ExprKind::FloatLiteral(_) if static_integral_float_array_key_value(expr).is_some() => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        ExprKind::Negate(inner)
            if matches!(inner.kind, ExprKind::IntLiteral(_))
                || static_integral_float_array_key_value(expr).is_some() =>
        {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        ExprKind::StringLiteral(_) => expr_is_eir_function_safe(expr, support, facts, scope_reads),
        _ => false,
    }
}

/// Returns true for parser-synthesized integer keys from unkeyed assoc entries.
pub(super) fn static_assoc_key_is_parser_generated(key: &Expr, value: &Expr) -> bool {
    matches!(key.kind, ExprKind::IntLiteral(_)) && key.span == value.span
}

/// Advances the static array auto-key cursor after an implicit generated key.
pub(super) fn advance_static_array_auto_key(next_auto_key: &mut i64, auto_key_initialized: &mut bool) {
    *next_auto_key = next_auto_key.saturating_add(1);
    *auto_key_initialized = true;
}

/// Updates the static array auto-key cursor from an explicit integer-like key.
pub(super) fn update_static_array_auto_key_from_explicit_key(
    key: &Expr,
    next_auto_key: &mut i64,
    auto_key_initialized: &mut bool,
) {
    if let Some(value) = static_integer_array_key_value(key) {
        let candidate = value.saturating_add(1);
        if !*auto_key_initialized || candidate > *next_auto_key {
            *next_auto_key = candidate;
        }
        *auto_key_initialized = true;
    }
}

/// Returns the integer value for static keys that affect PHP's next auto key.
pub(super) fn static_integer_array_key_value(key: &Expr) -> Option<i64> {
    match &key.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::BoolLiteral(value) => Some(i64::from(*value)),
        ExprKind::FloatLiteral(_) => static_integral_float_array_key_value(key),
        ExprKind::StringLiteral(value) if is_php_integer_array_key(value) => value.parse().ok(),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value.checked_neg(),
            ExprKind::FloatLiteral(_) => static_integral_float_array_key_value(key),
            _ => None,
        },
        _ => None,
    }
}

/// Returns the integer key for a float literal that PHP casts without a precision warning.
pub(super) fn static_integral_float_array_key_value(key: &Expr) -> Option<i64> {
    let value = match &key.kind {
        ExprKind::FloatLiteral(value) => *value,
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::FloatLiteral(value) => -*value,
            _ => return None,
        },
        _ => return None,
    };
    if !value.is_finite() || value.fract() != 0.0 {
        return None;
    }
    if value < i64::MIN as f64 || value >= i64::MAX as f64 {
        return None;
    }
    Some(value as i64)
}

/// Returns true when a static array value can be lowered without eval bridge state.
pub(super) fn expr_is_eir_static_array_value_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::Spread(_) => false,
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => {
            expr_is_eir_static_array_source_safe(expr, support, facts, scope_reads)
        }
        _ => expr_is_eir_function_safe(expr, support, facts, scope_reads),
    }
}
