//! Purpose:
//! Declarative eval registry entry and implementation for `proc_close`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem` direct and by-value dispatch.
//!
//! Key details:
//! - Closes any pipe the caller left open, waits for the child, and returns its exit code with
//!   php's wait semantics (`-1` when the child was already reaped by `proc_get_status()`).
//! - The process resource is marked closed, so `is_resource()` turns false afterwards.

eval_builtin! {
    contract: "proc_close",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for `proc_close` through the area dispatcher.
pub(in crate::interpreter) fn eval_proc_close_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [process] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let process = eval_expr(process, context, scope, values)?;
    eval_proc_close_result(process, context, values)
}

/// Dispatches evaluated-argument calls for `proc_close` through the area dispatcher.
pub(in crate::interpreter) fn eval_proc_close_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [process] => eval_proc_close_result(*process, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Closes a process handle and returns its exit code.
pub(in crate::interpreter) fn eval_proc_close_result(
    process: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = super::proc_open::eval_process_handle_id("proc_close", process, context, values)?;
    let code = context.stream_resources_mut().close_process(id).unwrap_or(-1);
    values.resource_closed(process)?;
    values.int(code)
}
