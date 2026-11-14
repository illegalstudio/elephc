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

/// Evaluates `stream_socket_pair($domain, $type, $protocol)`.
pub(in crate::interpreter) fn eval_stream_socket_pair_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(3..=4).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let _ = eval_expr(&args[0], context, scope, values)?;
    let _ = eval_expr(&args[1], context, scope, values)?;
    let _ = eval_expr(&args[2], context, scope, values)?;
    if let Some(context_arg) = args.get(3) {
        let _ = eval_expr(context_arg, context, scope, values)?;
    }
    eval_stream_socket_pair_result(context, values)
}

/// Creates a socket pair after validating already evaluated arguments.
pub(in crate::interpreter) fn eval_stream_socket_pair_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [_domain, _socket_type, _protocol] | [_domain, _socket_type, _protocol, _] => {
            eval_stream_socket_pair_result(context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
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
