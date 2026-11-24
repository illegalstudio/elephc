//! Purpose:
//! Declarative eval registry entry and implementation for `stream_socket_pair`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Creates connected local stream resources in eval's stream resource table.

eval_builtin! {
    contract: "stream_socket_pair",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;

/// Evaluates `stream_socket_pair($domain, $type, $protocol)`.
pub(in crate::interpreter) fn eval_stream_socket_pair_declared_call(
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

/// Creates a socket pair after validating already evaluated arguments.
pub(in crate::interpreter) fn eval_stream_socket_pair_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [_domain, _socket_type, _protocol] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
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
    let result = (|| {
        let mut result = EvalArrayBuilder::indexed(values, 2)?;
        result.index(0, |values| values.resource(left))?;
        result.index(1, |values| values.resource(right))?;
        Ok(result.finish())
    })();
    if result.is_err() {
        context.stream_resources_mut().close(left);
        context.stream_resources_mut().close(right);
    }
    result
}
