//! Purpose:
//! Declarative eval registry entry and implementation for `stream_socket_enable_crypto`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - TLS enablement is conservative: disabling succeeds for valid streams while
//!   enabling reports false because eval does not manage TLS state.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_socket_enable_crypto",
    area: Filesystem,
    params: [
        stream,
        enable,
        crypto_method = EvalBuiltinDefaultValue::Null,
        session_stream = EvalBuiltinDefaultValue::Null
    ],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_socket_enable_crypto($stream, $enable, ...)`.
pub(in crate::interpreter) fn eval_stream_socket_enable_crypto_declared_call(
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

/// Evaluates crypto status from already evaluated stream crypto arguments.
pub(in crate::interpreter) fn eval_stream_socket_enable_crypto_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&evaluated_args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_stream_socket_enable_crypto_result(evaluated_args[0], evaluated_args[1], context, values)
}

/// Returns TLS enablement status for eval socket streams.
pub(in crate::interpreter) fn eval_stream_socket_enable_crypto_result(
    stream: RuntimeCellHandle,
    enable: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = super::stream_socket_get_name::eval_socket_resource_id(stream, values)?;
    if !context.stream_resources().has_stream(id) {
        return values.bool_value(false);
    }
    let disabled = !values.truthy(enable)?;
    values.bool_value(disabled)
}
