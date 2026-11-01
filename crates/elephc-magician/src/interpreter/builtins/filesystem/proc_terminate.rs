//! Purpose:
//! Declarative eval registry entry and implementation for `proc_terminate`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem` direct and by-value dispatch.
//!
//! Key details:
//! - Sends the signal (default `SIGTERM`, 15) to the child; the child is not reaped, so a later
//!   `proc_get_status()` reports the signal.

eval_builtin! {
    contract: "proc_terminate",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for `proc_terminate` through the area dispatcher.
pub(in crate::interpreter) fn eval_proc_terminate_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=2).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let process = eval_expr(&args[0], context, scope, values)?;
    let signal = match args.get(1) {
        Some(signal) => Some(eval_expr(signal, context, scope, values)?),
        None => None,
    };
    eval_proc_terminate_result(process, signal, context, values)
}

/// Dispatches evaluated-argument calls for `proc_terminate` through the area dispatcher.
pub(in crate::interpreter) fn eval_proc_terminate_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [process] => eval_proc_terminate_result(*process, None, context, values),
        [process, signal] => eval_proc_terminate_result(*process, Some(*signal), context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Signals the child behind a process handle.
pub(in crate::interpreter) fn eval_proc_terminate_result(
    process: RuntimeCellHandle,
    signal: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id =
        super::proc_open::eval_process_handle_id("proc_terminate", process, context, values)?;
    let signal = match signal {
        Some(signal) => eval_int_value(signal, values)?,
        None => 15,
    };
    let delivered = elephc_proc::table::terminate(id, signal as i32).unwrap_or(false);
    values.bool_value(delivered)
}
