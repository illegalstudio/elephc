//! Purpose:
//! Evaluated-argument dispatch for declarative stream, socket, context, and CSV builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::values_dispatch`.
//!
//! Key details:
//! - By-value callable forms emit PHP-style warnings for parameters that are
//!   normally by-reference outputs.

use super::super::super::*;
use super::*;

/// Attempts evaluated-argument dispatch for stream and socket builtins.
pub(in crate::interpreter::builtins::filesystem) fn eval_filesystem_stream_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "fsockopen" | "pfsockopen" => {
            if !(2..=5).contains(&evaluated_args.len()) {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_fsockopen_by_value_ref_warnings(name, evaluated_args.len(), values)?;
            eval_fsockopen_result(evaluated_args[0], evaluated_args[1], context, values)?
        }
        "readline" => {
            if evaluated_args.len() > 1 {
                return Err(EvalStatus::RuntimeFatal);
            }
            let prompt = evaluated_args.first().copied();
            eval_readline_result(prompt, values)?
        }
        "stream_bucket_new" => {
            let [stream, buffer] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_bucket_new_result(*stream, *buffer, context, values)?
        }
        "stream_bucket_make_writeable" => {
            let [brigade] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_bucket_make_writeable_result(*brigade, values)?
        }
        "stream_bucket_append" | "stream_bucket_prepend" => {
            let [brigade, bucket] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_bucket_push_result(name, *brigade, *bucket, values)?
        }
        "stream_filter_register" => {
            let [filter_name, class] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_filter_register_result(*filter_name, *class, values)?
        }
        "stream_filter_append" | "stream_filter_prepend" => {
            if !(2..=4).contains(&evaluated_args.len()) {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_stream_filter_attach_result(
                name,
                evaluated_args[0],
                evaluated_args[1],
                context,
                values,
            )?
        }
        "stream_filter_remove" => {
            let [stream_filter] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_filter_remove_result(*stream_filter, context, values)?
        }
        "stream_select" => {
            eval_stream_select_by_value_ref_warnings(evaluated_args.len(), values)?;
            eval_stream_select_result(evaluated_args, context, values)?
        }
        "stream_socket_server" => {
            let [address] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_socket_server_result(*address, context, values)?
        }
        "stream_socket_client" => {
            let [address] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_socket_client_result(*address, context, values)?
        }
        "stream_socket_accept" => {
            if !(1..=3).contains(&evaluated_args.len()) {
                return Err(EvalStatus::RuntimeFatal);
            }
            if evaluated_args.len() >= 3 {
                values.warning(
                    "stream_socket_accept(): Argument #3 ($peer_name) must be passed by reference, value given",
                )?;
            }
            eval_stream_socket_accept_result(evaluated_args[0], context, values)?
        }
        "stream_socket_get_name" => {
            let [socket, remote] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_socket_get_name_result(*socket, *remote, context, values)?
        }
        "stream_socket_shutdown" => {
            let [stream, mode] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_socket_shutdown_result(*stream, *mode, context, values)?
        }
        "stream_socket_enable_crypto" => {
            if !(2..=4).contains(&evaluated_args.len()) {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_stream_socket_enable_crypto_result(
                evaluated_args[0],
                evaluated_args[1],
                context,
                values,
            )?
        }
        "stream_socket_sendto" => {
            if !(2..=4).contains(&evaluated_args.len()) {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_stream_socket_sendto_result(evaluated_args[0], evaluated_args[1], context, values)?
        }
        "stream_socket_recvfrom" => {
            if !(2..=4).contains(&evaluated_args.len()) {
                return Err(EvalStatus::RuntimeFatal);
            }
            if evaluated_args.len() >= 4 {
                values.warning(
                    "stream_socket_recvfrom(): Argument #4 ($address) must be passed by reference, value given",
                )?;
            }
            eval_stream_socket_recvfrom_result(
                evaluated_args[0],
                evaluated_args[1],
                context,
                values,
            )?
        }
        "stream_socket_pair" => {
            let [_domain, _socket_type, _protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_socket_pair_result(context, values)?
        }
        "stream_wrapper_register" | "stream_wrapper_unregister" | "stream_wrapper_restore" => {
            eval_stream_wrapper_registry_result(name, evaluated_args, context, values)?
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}

/// Emits PHP by-reference warnings for by-value `fsockopen()` error outputs.
fn eval_fsockopen_by_value_ref_warnings(
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

/// Emits PHP by-reference warnings for by-value `stream_select()` array outputs.
fn eval_stream_select_by_value_ref_warnings(
    supplied_count: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (index, param_name) in ["read", "write", "except"].iter().enumerate() {
        if supplied_count <= index {
            continue;
        }
        values.warning(&format!(
            "stream_select(): Argument #{} (${param_name}) must be passed by reference, value given",
            index + 1
        ))?;
    }
    Ok(())
}
