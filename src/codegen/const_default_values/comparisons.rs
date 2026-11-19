//! Purpose:
//! Resolves pure equality and identity comparisons in default expressions.
//!
//! Called from:
//! - `super::resolve_binary_expression_default()` for `==`, `!=`, `===` and `!==`.
//!
//! Key details:
//! - Enum singletons are identical only to themselves. Loose and strict comparison agree for two
//!   cases, and a case compared with a non-enum is false only under `===`/`!==`.
//! - Enum cases read by either operand stay ordered reads beside the boolean, as for branches.
//! - Loose comparisons that need PHP's numeric-string or cross-type rules decline.

use super::*;

/// Folds `left op right` into a bool when both operands resolve to comparable constants.
pub(super) fn resolve_equality_default(
    left: &Expr, op: &BinOp, right: &Expr, context: &ConstDefaultContext<'_>, depth: usize,
) -> Option<ConstDefaultValue> {
    let left = resolve_const_default_at(left, context, depth + 1)?;
    let right = resolve_const_default_at(right, context, depth + 1)?;
    let mut reads = Vec::new();
    collect_enum_reads(&left, &mut reads);
    collect_enum_reads(&right, &mut reads);
    let strict = matches!(op, BinOp::StrictEq | BinOp::StrictNotEq);
    let equal = values_equal(&without_enum_effects(left), &without_enum_effects(right), strict)?;
    let negated = matches!(op, BinOp::NotEq | BinOp::StrictNotEq);
    Some(with_enum_reads(const_default_bool(equal != negated), reads))
}

/// Compares two resolved scalars, strings and enum cases, or declines when PHP's rules are needed.
fn values_equal(left: &ConstDefaultValue, right: &ConstDefaultValue, strict: bool) -> Option<bool> {
    use ConstDefaultValue::{EnumCase, Scalar, String};
    match (left, right) {
        (EnumCase { .. }, EnumCase { .. }) => Some(left == right),
        (EnumCase { .. }, _) | (_, EnumCase { .. }) => strict.then_some(false),
        (String(left), String(right)) => {
            let same = elephc_builtin_contract::string_literal::literal_bytes(left)
                == elephc_builtin_contract::string_literal::literal_bytes(right);
            // Two different strings can still be loosely equal ("1e1" == "10").
            (strict || same).then_some(same)
        }
        (Scalar { kind: left_kind, payload: left_payload }, Scalar { kind: right_kind, payload: right_payload }) => {
            if left_kind != right_kind {
                return strict.then_some(false);
            }
            match *left_kind {
                CONST_DEFAULT_FLOAT => Some(
                    f64::from_bits(*left_payload as u64) == f64::from_bits(*right_payload as u64),
                ),
                CONST_DEFAULT_NULL | CONST_DEFAULT_BOOL | CONST_DEFAULT_INT | CONST_DEFAULT_EMPTY_ARRAY =>
                    Some(left_payload == right_payload),
                _ => None,
            }
        }
        (String(_), Scalar { .. }) | (Scalar { .. }, String(_)) => strict.then_some(false),
        _ => None,
    }
}
