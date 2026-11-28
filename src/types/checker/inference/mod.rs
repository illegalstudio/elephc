//! Purpose:
//! Coordinates expression and syntactic type inference for the checker.
//! Routes operators, objects, callables, assignments, and lightweight AST facts through one inference entry point.
//!
//! Called from:
//! - `crate::types::checker::Checker::infer_type()`
//!
//! Key details:
//! - Inference may emit diagnostics and warnings, so callers must preserve source spans and environment context.

mod expr;
pub(super) use expr::{ConditionalFlow, FlowSnapshot};
/// Inference for operators: binary ops, instanceof, closures, pipe, and expr calls.
mod objects;
/// Inference for objects: property access, method calls, constructors, and class constants.
mod ops;
/// Syntactic inference: lightweight AST-to-PhpType rules for literal-based return type hints.
/// Used as a fallback when precise type data is unavailable (e.g., unknown callables).
pub(super) mod syntactic;

pub use syntactic::{infer_expr_type_syntactic, infer_return_type_syntactic};

use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Returns the result type of a division between two integer literals.
///
/// PHP keeps an `int` only when the quotient is exact and fits; the `i64::MIN / -1`
/// overflow promotes to `float`, and a zero divisor stays a runtime error.
pub(super) fn checked_literal_int_division_type(left: &Expr, right: &Expr) -> Option<PhpType> {
    let lhs = match left.kind {
        ExprKind::IntLiteral(value) => value,
        _ => return None,
    };
    let rhs = match right.kind {
        ExprKind::IntLiteral(value) => value,
        _ => return None,
    };
    if rhs == 0 {
        return None;
    }
    if lhs == i64::MIN && rhs == -1 {
        return Some(PhpType::Float);
    }
    Some(if lhs % rhs == 0 { PhpType::Int } else { PhpType::Float })
}
pub(crate) use expr::closure_body_uses_this;
pub(super) use expr::is_valid_string_offset_index;
