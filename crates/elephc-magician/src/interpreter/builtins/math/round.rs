//! Purpose:
//! Eval registry entry and implementation for `round`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The optional precision defaults through registry metadata; direct calls
//!   still evaluate arguments in source order.

use super::super::super::*;
use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "round",
    area: Math,
    params: [num, precision = EvalBuiltinDefaultValue::Int(0)],
    direct: Round,
    values: Round,
}

/// Evaluates PHP `round()` over one value and an optional precision expression.
pub(in crate::interpreter) fn eval_builtin_round(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [num] => {
            let num = eval_expr(num, context, scope, values)?;
            eval_round_result(num, None, values)
        }
        [num, precision] => {
            let num = eval_expr(num, context, scope, values)?;
            let precision = eval_expr(precision, context, scope, values)?;
            eval_round_result(num, Some(precision), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Applies PHP `round()` to already evaluated arguments.
pub(in crate::interpreter) fn eval_round_result(
    num: RuntimeCellHandle,
    precision: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.round(num, precision)
}
