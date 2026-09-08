//! Purpose:
//! Releases proven expression temporaries after a borrowing consumer finishes.
//!
//! Called from:
//! - Expression operators and statement conditions.
//!
//! Key details:
//! - Variable loads and function returns are not presumed owning. Cleanup runs
//!   on success and failure without replacing an earlier evaluation error.

use super::*;

/// One evaluated expression and its proven cleanup obligation; false leaves legacy/borrowed values untouched.
#[derive(Clone, Copy, Debug)]
pub(in crate::interpreter) struct EvalExprResult {
    pub(in crate::interpreter) value: RuntimeCellHandle,
    pub(in crate::interpreter) owned: bool,
}

impl EvalExprResult {
    /// Wraps a result whose ownership has not been established at this boundary.
    pub(in crate::interpreter) fn unclassified(value: RuntimeCellHandle) -> Self {
        Self { value, owned: false }
    }
}

/// Identifies expressions whose runtime operations construct a fresh result cell.
pub(super) fn expression_owns_temporary(expr: &EvalExpr) -> bool {
    match expr {
        EvalExpr::Const(_) | EvalExpr::Binary { .. } => true,
        EvalExpr::Unary { op: EvalUnaryOp::Suppress, expr } => expression_owns_temporary(expr),
        EvalExpr::Unary { .. } => true,
        _ => false,
    }
}

/// Releases a proven evaluated owner after its consumer, preserving the original consumer error.
pub(in crate::interpreter) fn finish_evaluated_result<T>(
    value: EvalExprResult,
    result: Result<T, EvalStatus>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let cleanup = if value.owned { eval_release_value(context, values, value.value) } else { Ok(()) };
    result.and_then(|result| cleanup.map(|()| result))
}

/// Records fresh scalar argument cells; nested container aliases require a separate ownership proof.
pub(in crate::interpreter) fn record_scalar_argument_temporary(
    expr: &EvalExpr,
    value: RuntimeCellHandle,
    owners: &mut Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !expression_owns_temporary(expr) { return Ok(()); }
    let tag = match values.type_tag(value) {
        Ok(tag) => tag,
        Err(status) => {
            // Invocation cannot run after this failure, so even a container
            // owner is safe to release through the call's error cleanup.
            owners.push(value);
            return Err(status);
        }
    };
    if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT) {
        owners.push(value);
    }
    Ok(())
}

/// Evaluates a boolean condition once and releases its proven temporary result.
pub(in crate::interpreter) fn eval_condition(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let value = eval_expr_result(expr, context, scope, values)?;
    let result = values.truthy(value.value);
    finish_evaluated_result(value, result, context, values)
}
