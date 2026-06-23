//! Purpose:
//! Implements eval process pipe builtins `popen()` and `pclose()`.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_positional_expr_call()`.
//! - Dynamic callable dispatch under `builtins::registry::dispatch`.
//!
//! Key details:
//! - Process pipes are stored as normal eval stream resources plus a child
//!   process table entry so existing fread/fwrite paths work unchanged.
//! - `pclose()` removes the stream first, then waits for the child exit status.

use super::super::super::*;
use super::*;

/// Evaluates `popen(command, mode)`.
pub(in crate::interpreter) fn eval_builtin_popen(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [command, mode] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let command = eval_expr(command, context, scope, values)?;
    let mode = eval_expr(mode, context, scope, values)?;
    eval_popen_result(command, mode, context, values)
}

/// Opens a shell process pipe and returns a stream resource or false.
pub(in crate::interpreter) fn eval_popen_result(
    command: RuntimeCellHandle,
    mode: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let command = eval_path_string(command, values)?;
    let mode = eval_process_pipe_mode(mode, values)?;
    match context
        .stream_resources_mut()
        .open_process_pipe(&command, &mode)
    {
        Some(id) => values.resource(id),
        None => values.bool_value(false),
    }
}

/// Evaluates `pclose(handle)`.
pub(in crate::interpreter) fn eval_builtin_pclose(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [handle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let handle = eval_expr(handle, context, scope, values)?;
    eval_pclose_result(handle, context, values)
}

/// Closes a process pipe and returns its exit code, or false for invalid handles.
pub(in crate::interpreter) fn eval_pclose_result(
    handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_process_pipe_resource_id(handle, values)?;
    match context.stream_resources_mut().pclose(id) {
        Some(status) => values.int(status),
        None => values.bool_value(false),
    }
}

/// Reads a `popen()` mode string, accepting read or write pipe modes.
fn eval_process_pipe_mode(
    mode: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let mode = values.string_bytes(mode)?;
    let mode = String::from_utf8(mode).map_err(|_| EvalStatus::RuntimeFatal)?;
    match mode.chars().next() {
        Some('r' | 'w') => Ok(mode),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Converts a runtime resource cell into eval's zero-based process-pipe id.
fn eval_process_pipe_resource_id(
    handle: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(handle)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    let display_id = eval_int_value(handle, values)?;
    display_id.checked_sub(1).ok_or(EvalStatus::RuntimeFatal)
}
