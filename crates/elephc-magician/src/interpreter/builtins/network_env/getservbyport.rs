//! Purpose:
//! Eval registry entry and implementation for `getservbyport`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - C-string conversion and service-name extraction delegate to the owner builtin files.

use super::*;

eval_builtin! {
    name: "getservbyport",
    area: NetworkEnv,
    params: [port, protocol],
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `getservbyport($port, $protocol)` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_getservbyport(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [port, protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let port = eval_expr(port, context, scope, values)?;
    let protocol = eval_expr(protocol, context, scope, values)?;
    eval_getservbyport_result(port, protocol, values)
}

/// Looks up an internet service name by port and protocol.
pub(in crate::interpreter) fn eval_getservbyport_result(
    port: RuntimeCellHandle,
    protocol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let port = eval_int_value(port, values)?;
    let Ok(port) = u16::try_from(port) else {
        return values.bool_value(false);
    };
    let Some(protocol) = eval_lowercase_c_string(protocol, values)? else {
        return values.bool_value(false);
    };
    eval_servent_name_or_false(eval_service_name(port, &protocol), values)
}
