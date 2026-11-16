//! Purpose:
//! Declarative eval registry entry and implementation for `stream_socket_server`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Opened listeners enter eval's normal stream table.

eval_builtin! {
    contract: "stream_socket_server",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Evaluates `stream_socket_server($address)`.
pub(in crate::interpreter) fn eval_stream_socket_server_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [address] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let address = eval_expr(address, context, scope, values)?;
    eval_stream_socket_server_result(address, context, values)
}

/// Opens a listener from an already evaluated address argument.
pub(in crate::interpreter) fn eval_stream_socket_server_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [address] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_stream_socket_server_result(*address, context, values)
}

/// Opens a TCP listener resource.
pub(in crate::interpreter) fn eval_stream_socket_server_result(
    address: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let address = eval_path_string(address, values)?;
    match context.stream_resources_mut().open_tcp_listener(&address) {
        Some(id) => values.resource(id),
        None => values.bool_value(false),
    }
}

/// Evaluates `stream_socket_server()` over full eval call metadata, honouring its out-parameters.
///
/// `$error_code` and `$error_message` are written through the caller's own cells, as php does:
/// `0` and `""` on success, the OS errno and its plain `strerror()` text on failure.
pub(in crate::interpreter) fn eval_builtin_stream_socket_server_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |evaluated_args, context, _, values| {
        let (bound, _) = bind_evaluated_ref_builtin_args(
            &["address", "error_code", "error_message", "flags", "context"],
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
        let address = eval_path_string(address.value, values)?;
        let (result, error_code, error_message) =
            match context.stream_resources_mut().open_tcp_listener_result(&address) {
                Ok(id) => (values.resource(id)?, 0, String::new()),
                Err(error) => (
                    values.bool_value(false)?,
                    i64::from(error.raw_os_error().unwrap_or(0)),
                    super::fsockopen::eval_socket_error_message(&error),
                ),
            };
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
