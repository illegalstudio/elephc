//! Purpose:
//! Eval registry entry and implementation for `log`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The optional base defaults through registry metadata; direct calls still
//!   preserve source-order argument evaluation.

use super::super::super::*;
use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "log",
    area: Math,
    params: [num, base = EvalBuiltinDefaultValue::Float(std::f64::consts::E)],
    direct: Log,
    values: Log,
}

/// Evaluates PHP `log()` over one value and an optional base expression.
pub(in crate::interpreter) fn eval_builtin_log(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [num] => {
            let num = eval_expr(num, context, scope, values)?;
            eval_log_result(num, None, values)
        }
        [num, base] => {
            let num = eval_expr(num, context, scope, values)?;
            let base = eval_expr(base, context, scope, values)?;
            eval_log_result(num, Some(base), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Applies PHP `log()` to already evaluated arguments.
pub(in crate::interpreter) fn eval_log_result(
    num: RuntimeCellHandle,
    base: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let num = eval_float_value(num, values)?;
    let result = match base {
        Some(base) => num.log(eval_float_value(base, values)?),
        None => num.ln(),
    };
    values.float(result)
}
