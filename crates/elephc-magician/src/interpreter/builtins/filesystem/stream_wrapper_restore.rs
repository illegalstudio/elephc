//! Purpose:
//! Declarative eval registry entry and implementation for `stream_wrapper_restore`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Restores protocols in the eval stream wrapper registry.

eval_builtin! {
    name: "stream_wrapper_restore",
    area: Filesystem,
    params: [protocol],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_wrapper_restore($protocol)`.
pub(in crate::interpreter) fn eval_stream_wrapper_restore_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let protocol = eval_expr(protocol, context, scope, values)?;
    eval_stream_wrapper_restore_result(protocol, context, values)
}

/// Restores an already evaluated stream wrapper protocol.
pub(in crate::interpreter) fn eval_stream_wrapper_restore_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [protocol] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_stream_wrapper_restore_result(*protocol, context, values)
}

/// Restores a materialized stream wrapper protocol.
pub(in crate::interpreter) fn eval_stream_wrapper_restore_result(
    protocol: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let protocol = super::stream_wrapper_register::eval_stream_wrapper_protocol(protocol, values)?;
    values.bool_value(
        context
            .stream_resources_mut()
            .restore_stream_wrapper(&protocol, EVAL_STREAM_WRAPPERS),
    )
}
