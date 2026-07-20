//! Purpose:
//! Eval registry entry and alias implementation for `die`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core`.
//!
//! Key details:
//! - `die` shares `exit`'s process-status coercion and process termination.

use super::exit::{eval_exit_status_value, eval_process_exit};
use super::super::super::*;
use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "die",
    area: Core,
    params: [status = EvalBuiltinDefaultValue::Int(0)],
    direct: Core,
    values: Core,
}

/// Evaluates direct `die` calls from unevaluated EvalIR arguments.
pub(in crate::interpreter) fn eval_builtin_die(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let status = match args {
        [] => 0,
        [status] => {
            let status = eval_expr(status, context, scope, values)?;
            eval_int_value(status, values)?
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_process_exit(status)
}

/// Evaluates by-value `die` calls from already materialized arguments.
pub(in crate::interpreter) fn eval_die_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let status = eval_exit_status_value(evaluated_args, values)?;
    eval_process_exit(status)
}
