//! Purpose:
//! Declarative eval registry entry and implementation for `fsockopen`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//! - `crate::interpreter::expressions::eval_call()` for by-reference outputs.
//!
//! Key details:
//! - Direct calls keep their source-sensitive by-reference error-output path.
//! - `pfsockopen` shares this implementation because eval has no persistent sockets.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "fsockopen",
    area: Filesystem,
    params: [
        hostname,
        port,
        error_code: by_ref = EvalBuiltinDefaultValue::Null,
        error_message: by_ref = EvalBuiltinDefaultValue::Null,
        timeout = EvalBuiltinDefaultValue::Null
    ],
    by_ref: [error_code, error_message],
    direct: none,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Evaluates a positional `fsockopen()` call without writable error outputs.
pub(in crate::interpreter) fn eval_fsockopen_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=5).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let host = eval_expr(&args[0], context, scope, values)?;
    let port = eval_expr(&args[1], context, scope, values)?;
    for arg in &args[2..] {
        eval_expr(arg, context, scope, values)?;
    }
    eval_fsockopen_by_value_ref_warnings("fsockopen", args.len(), values)?;
    eval_fsockopen_result(host, port, context, values)
}

/// Evaluates a by-value `fsockopen()` call from already evaluated arguments.
pub(in crate::interpreter) fn eval_fsockopen_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=5).contains(&evaluated_args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_fsockopen_by_value_ref_warnings("fsockopen", evaluated_args.len(), values)?;
    eval_fsockopen_result(evaluated_args[0], evaluated_args[1], context, values)
}

/// Evaluates `fsockopen()` or `pfsockopen()` over full eval call metadata.
pub(in crate::interpreter) fn eval_builtin_fsockopen_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    let (bound, _) = bind_evaluated_ref_builtin_args(
        &["hostname", "port", "error_code", "error_message", "timeout"],
        &evaluated_args,
        false,
    )?;
    let host = required_evaluated_ref_arg(&bound, 0)?;
    let port = required_evaluated_ref_arg(&bound, 1)?;
    let error_code_target = optional_evaluated_ref_arg(&bound, 2)
        .map(|arg| arg.ref_target.clone().ok_or(EvalStatus::RuntimeFatal))
        .transpose()?;
    let error_message_target = optional_evaluated_ref_arg(&bound, 3)
        .map(|arg| arg.ref_target.clone().ok_or(EvalStatus::RuntimeFatal))
        .transpose()?;
    let (result, error_code, error_message) =
        eval_fsockopen_with_error_result(host.value, port.value, context, values)?;
    eval_write_socket_int_output_ref_target(
        error_code_target.as_ref(),
        error_code,
        context,
        values,
    )?;
    eval_write_socket_output_ref_target(
        error_message_target.as_ref(),
        Some(error_message),
        context,
        values,
    )?;
    Ok(result)
}

/// Opens a connected TCP stream from host and port cells.
pub(in crate::interpreter) fn eval_fsockopen_result(
    host: RuntimeCellHandle,
    port: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let host = eval_path_string(host, values)?;
    let port = eval_int_value(port, values)?;
    match context
        .stream_resources_mut()
        .open_tcp_stream_host_port(&host, port)
    {
        Some(id) => values.resource(id),
        None => values.bool_value(false),
    }
}

/// Opens a host/port TCP stream and returns PHP `fsockopen()` error outputs.
pub(in crate::interpreter) fn eval_fsockopen_with_error_result(
    host: RuntimeCellHandle,
    port: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, i64, String), EvalStatus> {
    let host = eval_path_string(host, values)?;
    let port = eval_int_value(port, values)?;
    match context
        .stream_resources_mut()
        .open_tcp_stream_host_port_result(&host, port)
    {
        Ok(id) => Ok((values.resource(id)?, 0, String::new())),
        Err(error) => {
            let error_code = i64::from(error.raw_os_error().unwrap_or(0));
            Ok((values.bool_value(false)?, error_code, error.to_string()))
        }
    }
}

/// Emits PHP by-reference warnings for by-value socket error outputs.
pub(in crate::interpreter) fn eval_fsockopen_by_value_ref_warnings(
    name: &str,
    supplied_count: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if supplied_count >= 3 {
        values.warning(&format!(
            "{name}(): Argument #3 ($error_code) must be passed by reference, value given"
        ))?;
    }
    if supplied_count >= 4 {
        values.warning(&format!(
            "{name}(): Argument #4 ($error_message) must be passed by reference, value given"
        ))?;
    }
    Ok(())
}

/// Writes a socket output string to a captured by-reference target when available.
pub(in crate::interpreter) fn eval_write_socket_output_ref_target(
    target: Option<&EvalReferenceTarget>,
    value: Option<String>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some((target, value)) = target.zip(value) else {
        return Ok(());
    };
    let value = values.string(&value)?;
    eval_write_direct_ref_target(
        target,
        value,
        context,
        values,
        Some(ScopeCellOwnership::Owned),
    )
}

/// Writes a socket output integer to a captured by-reference target when available.
pub(in crate::interpreter) fn eval_write_socket_int_output_ref_target(
    target: Option<&EvalReferenceTarget>,
    value: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(target) = target else {
        return Ok(());
    };
    let value = values.int(value)?;
    eval_write_direct_ref_target(
        target,
        value,
        context,
        values,
        Some(ScopeCellOwnership::Owned),
    )
}
