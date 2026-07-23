//! Purpose:
//! Declarative eval registry entry and process-resource implementation for `proc_close`.
//!
//! Called from:
//! - Eval builtin registry filesystem dispatch.
//!
//! Key details:
//! - Only process resources created by eval `proc_open` are accepted and waited.

eval_builtin! {
    name: "proc_close",
    area: Filesystem,
    params: [process],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates a direct `proc_close(process)` call.
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

/// Evaluates `proc_close` from normalized argument values.
pub(in crate::interpreter) fn eval_proc_close_declared_values_result(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [process] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_proc_close_result(*process, context, values)
}

/// Waits for an eval process resource and returns its child exit status.
fn eval_proc_close_result(
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
    match context.stream_resources_mut().close_process(id) {
        Some(status) => values.int(status),
        None => values.bool_value(false),
    }
}
