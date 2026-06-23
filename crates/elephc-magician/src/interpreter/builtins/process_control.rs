//! Purpose:
//! Implements eval-side process termination language constructs.
//!
//! Called from:
//! - `crate::interpreter::expressions` direct builtin dispatch.
//! - `crate::interpreter::builtins::registry::dispatch` for dynamic callable dispatch.
//!
//! Key details:
//! - `exit` and `die` match elephc's compiled behavior by terminating the host process.
//! - Tests must avoid executing these helpers directly because they do not return.

use super::super::*;
use super::*;

/// Evaluates direct `exit` or `die` calls from unevaluated EvalIR arguments.
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

/// Evaluates dynamic `exit` or `die` calls from already materialized arguments.
pub(in crate::interpreter) fn eval_exit_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let status = match evaluated_args {
        [] => 0,
        [status] => eval_int_value(*status, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_process_exit(status)
}

/// Terminates the current process with the PHP integer status clamped to `i32`.
fn eval_process_exit(status: i64) -> Result<RuntimeCellHandle, EvalStatus> {
    let status = i32::try_from(status).unwrap_or_else(|_| {
        if status.is_negative() {
            i32::MIN
        } else {
            i32::MAX
        }
    });
    std::process::exit(status)
}
