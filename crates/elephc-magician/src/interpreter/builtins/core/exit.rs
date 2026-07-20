//! Purpose:
//! Eval registry entry and implementation for `exit`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core`.
//!
//! Key details:
//! - The helper terminates the host process to match elephc's compiled behavior.

use super::super::super::*;
use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "exit",
    area: Core,
    params: [status = EvalBuiltinDefaultValue::Int(0)],
    direct: Core,
    values: Core,
}

/// Evaluates direct `exit` calls from unevaluated EvalIR arguments.
pub(in crate::interpreter) fn eval_builtin_exit(
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

/// Evaluates by-value `exit` calls from already materialized arguments.
pub(in crate::interpreter) fn eval_exit_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let status = eval_exit_status_value(evaluated_args, values)?;
    eval_process_exit(status)
}

/// Reads the optional PHP integer process status for `exit` and `die`.
pub(in crate::interpreter) fn eval_exit_status_value(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    match evaluated_args {
        [] => Ok(0),
        [status] => eval_int_value(*status, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Terminates the current process with the PHP integer status clamped to `i32`.
pub(in crate::interpreter) fn eval_process_exit(
    status: i64,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let status = i32::try_from(status).unwrap_or_else(|_| {
        if status.is_negative() {
            i32::MIN
        } else {
            i32::MAX
        }
    });
    std::process::exit(status)
}
