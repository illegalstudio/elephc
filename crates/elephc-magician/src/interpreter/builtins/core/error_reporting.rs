//! Purpose:
//! Eval registry entry and implementation for PHP's `error_reporting`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Omitted and explicit-null levels only query the shared runtime mask.
//! - Integer levels update that mask and return its previous value.

use super::super::super::*;
eval_builtin! {
    contract: "error_reporting",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates a direct PHP `error_reporting()` query or update.
pub(in crate::interpreter) fn eval_builtin_error_reporting(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let level = match args {
        [] => None,
        [level] => {
            let level = eval_expr(level, context, scope, values)?;
            if values.is_null(level)? {
                None
            } else {
                Some(eval_int_value(level, values)?)
            }
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_error_reporting_result(level, values)
}

/// Evaluates a by-value PHP `error_reporting()` query or update.
pub(in crate::interpreter) fn eval_error_reporting_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let level = match evaluated_args {
        [] => None,
        [level] if values.is_null(*level)? => None,
        [level] => Some(eval_int_value(*level, values)?),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_error_reporting_result(level, values)
}

/// Returns the previous shared error mask after optionally replacing it.
fn eval_error_reporting_result(
    level: Option<i64>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let previous = values.error_reporting(level)?;
    values.int(previous)
}
