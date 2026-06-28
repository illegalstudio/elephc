//! Purpose:
//! Implements eval stream socket builtins over host TCP and local socket pairs.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_positional_expr_call()`.
//! - Dynamic callable dispatch under `builtins::registry::dispatch`.
//!
//! Key details:
//! - TCP streams are inserted into eval's normal File-backed stream table so
//!   existing fread/fwrite/close paths keep working.
//! - TLS enablement is conservative: disabling succeeds for valid streams,
//!   enabling returns false because eval does not own TLS state.

use super::super::super::*;
use super::*;

/// Evaluates `stream_socket_server(address)`.
pub(in crate::interpreter) fn eval_builtin_stream_socket_server(
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

/// Evaluates `stream_socket_client(address)`.
pub(in crate::interpreter) fn eval_builtin_stream_socket_client(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [address] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let address = eval_expr(address, context, scope, values)?;
    eval_stream_socket_client_result(address, context, values)
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

/// Evaluates `fsockopen()` or `pfsockopen()`.
pub(in crate::interpreter) fn eval_builtin_fsockopen(
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
        let _ = eval_expr(arg, context, scope, values)?;
    }
    eval_fsockopen_result(host, port, context, values)
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

/// Evaluates `stream_socket_accept(socket, timeout = null, peer_name = null)`.
pub(in crate::interpreter) fn eval_builtin_stream_socket_accept(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=3).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let socket = eval_expr(&args[0], context, scope, values)?;
    for arg in &args[1..] {
        let _ = eval_expr(arg, context, scope, values)?;
    }
    eval_stream_socket_accept_result(socket, context, values)
}

/// Accepts one pending TCP connection from a listener resource.
pub(in crate::interpreter) fn eval_stream_socket_accept_result(
    socket: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_socket_resource_id(socket, values)?;
    match context.stream_resources_mut().accept_tcp(id) {
        Some(id) => values.resource(id),
        None => values.bool_value(false),
    }
}

/// Evaluates `stream_socket_get_name(socket, remote)`.
pub(in crate::interpreter) fn eval_builtin_stream_socket_get_name(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [socket, remote] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let socket = eval_expr(socket, context, scope, values)?;
    let remote = eval_expr(remote, context, scope, values)?;
    eval_stream_socket_get_name_result(socket, remote, context, values)
}

/// Returns a tracked local or remote socket endpoint name.
pub(in crate::interpreter) fn eval_stream_socket_get_name_result(
    socket: RuntimeCellHandle,
    remote: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_socket_resource_id(socket, values)?;
    let remote = values.truthy(remote)?;
    match context.stream_resources().socket_name(id, remote) {
        Some(name) => values.string(&name),
        None => values.bool_value(false),
    }
}

/// Evaluates `stream_socket_shutdown(stream, mode)`.
pub(in crate::interpreter) fn eval_builtin_stream_socket_shutdown(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream, mode] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    let mode = eval_expr(mode, context, scope, values)?;
    eval_stream_socket_shutdown_result(stream, mode, context, values)
}

/// Applies a socket shutdown mode to a stream resource.
pub(in crate::interpreter) fn eval_stream_socket_shutdown_result(
    stream: RuntimeCellHandle,
    mode: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_socket_resource_id(stream, values)?;
    let mode = eval_int_value(mode, values)?;
    values.bool_value(
        context
            .stream_resources()
            .socket_shutdown(id, mode)
            .unwrap_or(false),
    )
}

/// Evaluates `stream_socket_enable_crypto(...)`.
pub(in crate::interpreter) fn eval_builtin_stream_socket_enable_crypto(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let enable = eval_expr(&args[1], context, scope, values)?;
    for arg in &args[2..] {
        let _ = eval_expr(arg, context, scope, values)?;
    }
    eval_stream_socket_enable_crypto_result(stream, enable, context, values)
}

/// Returns TLS enablement status for eval socket streams.
pub(in crate::interpreter) fn eval_stream_socket_enable_crypto_result(
    stream: RuntimeCellHandle,
    enable: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_socket_resource_id(stream, values)?;
    if !context.stream_resources().has_stream(id) {
        return values.bool_value(false);
    }
    let disabled = !values.truthy(enable)?;
    values.bool_value(disabled)
}

/// Evaluates `stream_socket_sendto(stream, data, flags = 0, address = null)`.
pub(in crate::interpreter) fn eval_builtin_stream_socket_sendto(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let data = eval_expr(&args[1], context, scope, values)?;
    for arg in &args[2..] {
        let _ = eval_expr(arg, context, scope, values)?;
    }
    eval_stream_socket_sendto_result(stream, data, context, values)
}

/// Writes bytes to a connected socket stream.
pub(in crate::interpreter) fn eval_stream_socket_sendto_result(
    stream: RuntimeCellHandle,
    data: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_fwrite_result(stream, data, context, values)
}

/// Evaluates `stream_socket_recvfrom(stream, length, flags = 0, address = null)`.
pub(in crate::interpreter) fn eval_builtin_stream_socket_recvfrom(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let length = eval_expr(&args[1], context, scope, values)?;
    for arg in &args[2..] {
        let _ = eval_expr(arg, context, scope, values)?;
    }
    eval_stream_socket_recvfrom_result(stream, length, context, values)
}

/// Reads bytes from a connected socket stream.
pub(in crate::interpreter) fn eval_stream_socket_recvfrom_result(
    stream: RuntimeCellHandle,
    length: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_fread_result(stream, length, context, values)
}

/// Evaluates `stream_socket_pair(domain, type, protocol)`.
pub(in crate::interpreter) fn eval_builtin_stream_socket_pair(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [domain, socket_type, protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let _ = eval_expr(domain, context, scope, values)?;
    let _ = eval_expr(socket_type, context, scope, values)?;
    let _ = eval_expr(protocol, context, scope, values)?;
    eval_stream_socket_pair_result(context, values)
}

/// Creates a pair of connected local stream resources.
pub(in crate::interpreter) fn eval_stream_socket_pair_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((left, right)) = context.stream_resources_mut().open_socket_pair() else {
        return values.bool_value(false);
    };
    let mut result = values.array_new(2)?;
    let key = values.int(0)?;
    let value = values.resource(left)?;
    result = values.array_set(result, key, value)?;
    let key = values.int(1)?;
    let value = values.resource(right)?;
    values.array_set(result, key, value)
}

/// Evaluates `stream_select(...)` as a conservative non-blocking readiness check.
pub(in crate::interpreter) fn eval_builtin_stream_select(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(4..=5).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_stream_select_result(&evaluated_args, context, values)
}

/// Evaluates materialized `stream_select(...)` arguments.
pub(in crate::interpreter) fn eval_stream_select_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(4..=5).contains(&evaluated_args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    for array in evaluated_args.iter().take(3) {
        eval_stream_select_cast_array(*array, context, values)?;
    }
    values.int(0)
}

/// Invokes `stream_cast(STREAM_CAST_FOR_SELECT)` for wrapper resources in an array.
fn eval_stream_select_cast_array(
    array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !values.is_array_like(array)? {
        return Ok(());
    }
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        eval_stream_select_cast_value(value, context, values)?;
    }
    Ok(())
}

/// Invokes `stream_cast()` for one userspace-wrapper stream resource value.
fn eval_stream_select_cast_value(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_RESOURCE {
        return Ok(());
    }
    let display_id = eval_int_value(value, values)?;
    let Some(id) = display_id.checked_sub(1) else {
        return Ok(());
    };
    let Some(result) =
        eval_user_wrapper_stream_cast_result(id, EVAL_STREAM_CAST_FOR_SELECT, context, values)?
    else {
        return Ok(());
    };
    values.release(result)
}

/// Converts a runtime resource cell into eval's zero-based socket id.
fn eval_socket_resource_id(
    resource: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(resource)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    let display_id = eval_int_value(resource, values)?;
    display_id.checked_sub(1).ok_or(EvalStatus::RuntimeFatal)
}
