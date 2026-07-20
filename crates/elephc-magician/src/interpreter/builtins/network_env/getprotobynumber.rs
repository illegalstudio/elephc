//! Purpose:
//! Eval registry entry and implementation for `getprotobynumber`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Protocol-name extraction delegates to `getprotobyname`.

use super::*;

eval_builtin! {
    name: "getprotobynumber",
    area: NetworkEnv,
    params: [protocol],
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `getprotobynumber($protocol)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_getprotobynumber(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let protocol = eval_expr(protocol, context, scope, values)?;
    eval_getprotobynumber_result(protocol, values)
}

/// Looks up an IP protocol name by numeric protocol id.
pub(in crate::interpreter) fn eval_getprotobynumber_result(
    protocol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let protocol = eval_int_value(protocol, values)?;
    let Ok(protocol) = libc::c_int::try_from(protocol) else {
        return values.bool_value(false);
    };
    let entry = unsafe {
        // libc returns a process-global protoent; copy the name before another lookup.
        libc_getprotobynumber(protocol)
    };
    eval_protoent_name_or_false(entry, values)
}
