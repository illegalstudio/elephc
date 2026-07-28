//! Purpose:
//! Declares and implements eval-time `proc_get_status` for `proc_open` resources.
//!
//! Called from:
//! - The filesystem builtin direct and normalized-value dispatchers.
//!
//! Key details:
//! - Status inspection is non-consuming so a later `proc_close` can still wait.

eval_builtin! {
    name: "proc_get_status",
    area: Filesystem,
    params: [process],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates a direct `proc_get_status(process)` call.
pub(in crate::interpreter) fn eval_proc_get_status_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [process] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let process = eval_expr(process, context, scope, values)?;
    eval_proc_get_status_result(process, context, values)
}

/// Evaluates `proc_get_status` from normalized argument values.
pub(in crate::interpreter) fn eval_proc_get_status_declared_values_result(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [process] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_proc_get_status_result(*process, context, values)
}

/// Builds PHP's process-status array for one live eval process resource.
fn eval_proc_get_status_result(
    process: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(process)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    let id = eval_int_value(process, values)?
        .checked_sub(1)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let Some(status) = context.stream_resources_mut().process_status(id) else {
        return values.bool_value(false);
    };
    let entries = [
        ("command", values.string(&status.command)?),
        ("pid", values.int(status.pid)?),
        ("cached", values.bool_value(status.cached)?),
        ("running", values.bool_value(status.running)?),
        ("signaled", values.bool_value(status.signaled)?),
        ("stopped", values.bool_value(status.stopped)?),
        ("exitcode", values.int(status.exitcode)?),
        ("termsig", values.int(status.termsig)?),
        ("stopsig", values.int(status.stopsig)?),
    ];
    let mut result = values.array_new(entries.len())?;
    for (name, value) in entries {
        let name = values.string(name)?;
        result = values.array_set(result, name, value)?;
    }
    Ok(result)
}
