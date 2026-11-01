//! Purpose:
//! Declarative eval registry entry and implementation for `stream_socket_client`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//! - `crate::interpreter::expressions::eval_call()` for by-reference outputs.
//!
//! Key details:
//! - Opened sockets enter eval's normal stream table.
//! - `$error_code` and `$error_message` are PHP out-parameters and are written on EVERY outcome
//!   (`0` and `''` on success), so a caller may read them unconditionally. The writable path
//!   needs the reference targets `eval_call` keeps, exactly as `fsockopen` does; the direct and
//!   by-value dispatchers still accept the wider arity and warn, because those two have already
//!   lost the caller's cells.

eval_builtin! {
    contract: "stream_socket_client",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// PHP's arity range for `stream_socket_client()`: address, two by-reference outputs, timeout,
/// flags and context.
const ARITY: std::ops::RangeInclusive<usize> = 1..=6;

/// Evaluates `stream_socket_client($address, ...)` without writable error outputs.
pub(in crate::interpreter) fn eval_stream_socket_client_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !ARITY.contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let address = eval_expr(&args[0], context, scope, values)?;
    for arg in &args[1..] {
        eval_expr(arg, context, scope, values)?;
    }
    eval_stream_socket_client_by_value_ref_warnings(args.len(), values)?;
    eval_stream_socket_client_result(address, context, values)
}

/// Opens a connected stream from already evaluated arguments, without writable error outputs.
pub(in crate::interpreter) fn eval_stream_socket_client_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !ARITY.contains(&evaluated_args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_stream_socket_client_by_value_ref_warnings(evaluated_args.len(), values)?;
    eval_stream_socket_client_result(evaluated_args[0], context, values)
}

/// Evaluates `stream_socket_client()` over full eval call metadata, honouring its out-parameters.
pub(in crate::interpreter) fn eval_builtin_stream_socket_client_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |evaluated_args, context, _, values| {
        let (bound, _) = bind_evaluated_ref_builtin_args(
            &[
                "address",
                "error_code",
                "error_message",
                "timeout",
                "flags",
                "context",
            ],
            &evaluated_args,
            false,
        )?;
        let address = required_evaluated_ref_arg(&bound, 0)?;
        let error_code_target = optional_evaluated_ref_arg(&bound, 1)
            .map(|arg| arg.ref_target.clone().ok_or(EvalStatus::RuntimeFatal))
            .transpose()?;
        let error_message_target = optional_evaluated_ref_arg(&bound, 2)
            .map(|arg| arg.ref_target.clone().ok_or(EvalStatus::RuntimeFatal))
            .transpose()?;
        let (result, error_code, error_message) =
            eval_stream_socket_client_with_error_result(address.value, context, values)?;
        super::fsockopen::eval_write_socket_int_output_ref_target(
            error_code_target.as_ref(),
            error_code,
            context,
            values,
        )?;
        super::fsockopen::eval_write_socket_output_ref_target(
            error_message_target.as_ref(),
            Some(error_message),
            context,
            values,
        )?;
        Ok(result)
    })
}

/// Opens a connected TCP stream resource.
pub(in crate::interpreter) fn eval_stream_socket_client_result(
    address: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let address = eval_path_string(address, values)?;
    match context.stream_resources_mut().open_tcp_stream(&address) {
        Some(id) => values.resource(id),
        None => values.bool_value(false),
    }
}

/// Opens a connected TCP stream and returns PHP `stream_socket_client()` error outputs with it.
pub(in crate::interpreter) fn eval_stream_socket_client_with_error_result(
    address: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, i64, String), EvalStatus> {
    let address = eval_path_string(address, values)?;
    match context.stream_resources_mut().open_tcp_stream_result(&address) {
        Ok(id) => Ok((values.resource(id)?, 0, String::new())),
        Err(error) => {
            let error_code = i64::from(error.raw_os_error().unwrap_or(0));
            Ok((values.bool_value(false)?, error_code, error.to_string()))
        }
    }
}

/// Emits PHP by-reference warnings for by-value socket error outputs.
fn eval_stream_socket_client_by_value_ref_warnings(
    supplied_count: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if supplied_count >= 2 {
        values.warning(
            "stream_socket_client(): Argument #2 ($error_code) must be passed by reference, \
             value given",
        )?;
    }
    if supplied_count >= 3 {
        values.warning(
            "stream_socket_client(): Argument #3 ($error_message) must be passed by reference, \
             value given",
        )?;
    }
    Ok(())
}
