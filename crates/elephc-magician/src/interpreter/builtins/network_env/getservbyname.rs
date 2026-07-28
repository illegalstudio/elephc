//! Purpose:
//! Eval registry entry and implementation for `getservbyname`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Service-name extraction is owned here and reused by `getservbyport`.

use super::*;

eval_builtin! {
    name: "getservbyname",
    area: NetworkEnv,
    params: [service, protocol],
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `getservbyname($service, $protocol)` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_getservbyname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [service, protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let service = eval_expr(service, context, scope, values)?;
    let protocol = eval_expr(protocol, context, scope, values)?;
    eval_getservbyname_result(service, protocol, values)
}

/// Looks up an internet service port by service name and protocol.
pub(in crate::interpreter) fn eval_getservbyname_result(
    service: RuntimeCellHandle,
    protocol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(service) = eval_lowercase_c_string(service, values)? else {
        return values.bool_value(false);
    };
    let Some(protocol) = eval_lowercase_c_string(protocol, values)? else {
        return values.bool_value(false);
    };
    match eval_service_port(&service, &protocol) {
        Some(port) => values.int(i64::from(port)),
        None => values.bool_value(false),
    }
}


/// Copies a service canonical name into a PHP string or returns PHP false.
pub(in crate::interpreter) fn eval_servent_name_or_false(
    name: Option<Vec<u8>>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        Some(name) => values.string_bytes_value(&name),
        None => values.bool_value(false),
    }
}
