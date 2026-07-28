//! Purpose:
//! Declares and implements eval-time process termination for `proc_open` resources.
//!
//! Called from:
//! - The filesystem builtin direct and normalized-value dispatchers.
//!
//! Key details:
//! - Unix forwards the requested signal while Windows uses the child termination API.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "proc_terminate",
    area: Filesystem,
    params: [process, signal = EvalBuiltinDefaultValue::Int(15)],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates a direct `proc_terminate(process, signal = 15)` call.
pub(in crate::interpreter) fn eval_proc_terminate_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (process, signal) = match args {
        [process] => (eval_expr(process, context, scope, values)?, 15),
        [process, signal] => (
            eval_expr(process, context, scope, values)?,
            eval_int_value(eval_expr(signal, context, scope, values)?, values)?,
        ),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_proc_terminate_result(process, signal, context, values)
}

/// Evaluates `proc_terminate` from normalized argument values.
pub(in crate::interpreter) fn eval_proc_terminate_declared_values_result(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (process, signal) = match args {
        [process] => (*process, 15),
        [process, signal] => (*process, eval_int_value(*signal, values)?),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_proc_terminate_result(process, signal, context, values)
}

/// Terminates one eval process resource and returns PHP's boolean success result.
fn eval_proc_terminate_result(
    process: RuntimeCellHandle,
    signal: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(process)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    let id = eval_int_value(process, values)?
        .checked_sub(1)
        .ok_or(EvalStatus::RuntimeFatal)?;
    values.bool_value(
        context
            .stream_resources_mut()
            .terminate_process(id, signal)
            .unwrap_or(false),
    )
}
