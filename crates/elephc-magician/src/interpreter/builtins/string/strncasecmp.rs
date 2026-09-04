//! Purpose:
//! Eval registry entry and implementation for PHP `strncasecmp`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks` for direct and callable eval dispatch.
//!
//! Key details:
//! - Comparison folds ASCII bytes only and delegates length validation to the
//!   shared bounded string-comparison implementation.

use super::super::super::*;

eval_builtin! {
    contract: "strncasecmp",
    area: String,
    direct: StringCompare,
    values: StringCompare,
}

/// Evaluates PHP `strncasecmp(...)` arguments in source order.
pub(in crate::interpreter) fn eval_builtin_strncasecmp(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right, length] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    let length = eval_expr(length, context, scope, values)?;
    eval_strncasecmp_result(left, right, length, context, values)
}

/// Applies PHP `strncasecmp(...)` to already evaluated values.
pub(in crate::interpreter) fn eval_strncasecmp_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    length: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::strncmp::eval_string_ncompare_named_result(
        "strncasecmp",
        left,
        right,
        length,
        context,
        values,
    )
}
