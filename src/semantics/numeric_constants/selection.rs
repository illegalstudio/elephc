//! Purpose:
//! Selects original constant scalar kinds through PHP lazy value expressions.
//!
//! Called from:
//! - Source constant evidence before folding or declaration compatibility can widen a result.
//!
//! Key details:
//! - String truthiness is retained independently of numeric parsing, including zero spellings.
//! - Unknown array truth or source operands do not manufacture a selected branch.

use super::*;

/// Proves only truth values represented exactly by the original scalar evidence.
fn truthy(value: Number) -> Option<bool> {
    match value {
        Number::Int(value) => Some(value != 0), Number::Float(value) => Some(value != 0.0),
        Number::Bool(value) => Some(value), Number::Null => Some(false),
        Number::StringInt { truthy, .. } | Number::StringFloat { truthy, .. }
        | Number::NonNumericString { truthy, .. } => Some(truthy),
        Number::Array => None,
    }
}

/// Evaluates only the selected value, preserving its PHP kind and lazy default semantics.
pub(super) fn evaluate_selection(expr: &Expr, checked: bool, class: Option<&str>,
    definitions: &Definitions, evidence: &mut Evidence) -> Result<Option<Number>, CompileError> {
    match &expr.kind {
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            let Some(condition) = evaluate(condition, checked, class, definitions, evidence)?.and_then(truthy)
                else { return Ok(None); };
            evaluate(if condition { then_expr } else { else_expr }, checked, class, definitions, evidence)
        }
        ExprKind::ShortTernary { value, default } => {
            let Some(value) = evaluate(value, checked, class, definitions, evidence)? else { return Ok(None); };
            match truthy(value) {
                Some(true) => Ok(Some(value)), Some(false) => evaluate(default, checked, class, definitions, evidence),
                None => Ok(None),
            }
        }
        ExprKind::NullCoalesce { value, default } => {
            match evaluate(value, checked, class, definitions, evidence)? {
                Some(Number::Null) => evaluate(default, checked, class, definitions, evidence),
                value => Ok(value),
            }
        }
        _ => unreachable!("only lazy selection expressions use this evaluator"),
    }
}
