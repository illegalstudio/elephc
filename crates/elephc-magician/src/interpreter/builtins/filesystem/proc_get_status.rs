//! Purpose:
//! Declarative eval registry entry and implementation for `proc_get_status`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem` direct and by-value dispatch.
//!
//! Key details:
//! - The array has php 8.5's nine keys in php's order. The status table in `elephc_proc` owns the
//!   wait semantics: a normal exit is cached and repeated, a signal death is reported once.

eval_builtin! {
    contract: "proc_get_status",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use crate::interpreter::collection_builder::EvalArrayBuilder;

/// Dispatches direct eval calls for `proc_get_status` through the area dispatcher.
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

/// Dispatches evaluated-argument calls for `proc_get_status` through the area dispatcher.
pub(in crate::interpreter) fn eval_proc_get_status_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [process] => eval_proc_get_status_result(*process, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Polls the child without blocking and builds the status array.
pub(in crate::interpreter) fn eval_proc_get_status_result(
    process: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id =
        super::proc_open::eval_process_handle_id("proc_get_status", process, context, values)?;
    let (Some(snapshot), Some((pid, command))) = (
        elephc_proc::table::poll(id),
        elephc_proc::table::describe(id),
    ) else {
        return values.bool_value(false);
    };
    let mut result = EvalArrayBuilder::assoc(values, 9)?;
    result.string("command", |values| values.string(&command))?;
    result.string("pid", |values| values.int(i64::from(pid)))?;
    result.string("cached", |values| values.bool_value(snapshot.cached))?;
    result.string("running", |values| values.bool_value(snapshot.running))?;
    result.string("signaled", |values| values.bool_value(snapshot.signaled))?;
    result.string("stopped", |values| values.bool_value(snapshot.stopped))?;
    result.string("exitcode", |values| values.int(snapshot.exitcode))?;
    result.string("termsig", |values| values.int(snapshot.termsig))?;
    result.string("stopsig", |values| values.int(snapshot.stopsig))?;
    Ok(result.finish())
}
