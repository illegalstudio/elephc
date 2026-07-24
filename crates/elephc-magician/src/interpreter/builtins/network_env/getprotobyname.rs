//! Purpose:
//! Eval registry entry and implementation for `getprotobyname`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Lowercase C-string and protoent-name helpers are owned here for protocol lookups.

use super::*;

eval_builtin! {
    name: "getprotobyname",
    area: NetworkEnv,
    params: [protocol],
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `getprotobyname($protocol)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_getprotobyname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let protocol = eval_expr(protocol, context, scope, values)?;
    eval_getprotobyname_result(protocol, values)
}

/// Looks up an IP protocol number by name or alias.
pub(in crate::interpreter) fn eval_getprotobyname_result(
    protocol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(protocol) = eval_lowercase_c_string(protocol, values)? else {
        return values.bool_value(false);
    };
    match eval_protocol_number(&protocol) {
        Some(number) => values.int(i64::from(number)),
        None => values.bool_value(false),
    }
}


/// Converts a PHP value to a NUL-free lowercase C string for libc database lookups.
pub(in crate::interpreter) fn eval_lowercase_c_string(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<CString>, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let bytes = bytes
        .into_iter()
        .map(|byte| byte.to_ascii_lowercase())
        .collect::<Vec<_>>();
    Ok(CString::new(bytes).ok())
}

/// Copies a protocol canonical name into a PHP string or returns PHP false.
pub(in crate::interpreter) fn eval_protoent_name_or_false(
    name: Option<Vec<u8>>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        Some(name) => values.string_bytes_value(&name),
        None => values.bool_value(false),
    }
}
