//! Purpose:
//! Resolves pure property-default branch selectors for lazy reflection metadata.
//!
//! Called from:
//! - Property slot default metadata construction in the reflection backend.
//!
//! Key details:
//! - The shared constant resolver supplies PHP truthiness and lexical class lookup.
//! - Deferred Errors remain data and only a selected branch may become a root Error.

use super::*;
use crate::codegen::const_default_values::{self, ConstDefaultContext};

/// Folds known scalar constants before selecting a property's lazy value or Error metadata.
pub(super) fn reflection_folded_property_default(
    ctx: &FunctionContext<'_>, declaring_class: Option<&str>, default: &Expr,
) -> Option<ReflectionParameterDefaultValue> {
    let context = ConstDefaultContext { module: ctx.module, current_class: declaring_class };
    let mut default = default.clone();
    fold(&mut default, &context);
    crate::types::deferred_defaults::prune_literal_branches(&mut default);
    crate::types::deferred_defaults::unconditional_error_message(&default)
        .map(|message| ReflectionParameterDefaultValue::DeferredError(message.to_string()))
        .or_else(|| reflection_literal_parameter_default_value(&default))
}

/// Recurses over constant-expression operands without evaluating a deferred Error.
fn fold(expr: &mut Expr, context: &ConstDefaultContext<'_>) {
    match &mut expr.kind {
        ExprKind::ArrayLiteral(values) => {
            for value in values { fold(value, context); }
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            for (key, value) in entries { fold(key, context); fold(value, context); }
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries { for value in entry.exprs_mut() { fold(value, context); } }
        }
        ExprKind::BinaryOp { left, right, .. } => { fold(left, context); fold(right, context); }
        ExprKind::ArrayAccess { array, index } => { fold(array, context); fold(index, context); }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            fold(condition, context); fold(then_expr, context); fold(else_expr, context);
            if let Some(truthy) = const_default_values::resolve_const_default(condition, context)
                .and_then(|value| const_default_values::const_default_truthy(&value))
            {
                *expr = if truthy { *then_expr.clone() } else { *else_expr.clone() };
            }
        }
        ExprKind::NullCoalesce { value, default } => {
            fold(value, context); fold(default, context);
            if let Some(resolved) = const_default_values::resolve_const_default(value, context) {
                *expr = if const_default_values::const_default_is_null(&resolved) {
                    *default.clone()
                } else { *value.clone() };
            }
        }
        ExprKind::ShortTernary { value, default } => {
            fold(value, context); fold(default, context);
            if let Some(truthy) = const_default_values::resolve_const_default(value, context)
                .and_then(|value| const_default_values::const_default_truthy(&value))
            {
                *expr = if truthy { *value.clone() } else { *default.clone() };
            }
        }
        ExprKind::Match { subject, arms, default } => {
            fold(subject, context);
            for (conditions, result) in arms {
                for condition in conditions { fold(condition, context); }
                fold(result, context);
            }
            if let Some(default) = default { fold(default, context); }
        }
        ExprKind::Negate(inner) | ExprKind::Not(inner) | ExprKind::BitNot(inner)
        | ExprKind::Spread(inner) | ExprKind::Cast { expr: inner, .. } => fold(inner, context),
        _ => {}
    }
    if let Some(value) = const_default_values::resolve_const_default(expr, context)
        .and_then(const_default_values::resolved_scalar_literal)
    {
        expr.kind = value;
    }
}
