//! Purpose:
//! Declarative eval registry entry for `fgets`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the unary stream helper.

eval_builtin! {
    name: "fgets",
    area: Filesystem,
    params: [stream],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `fgets` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fgets_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_fgets(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fgets` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fgets_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream] => eval_fgets_result(*stream, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `fgets($stream)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_fgets(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    eval_fgets_result(stream, context, values)
}

/// Reads one newline-terminated string from a materialized stream resource.
pub(in crate::interpreter) fn eval_fgets_result(
    stream: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    if let Some(result) = eval_user_wrapper_fgets_result(id, context, values)? {
        return Ok(result);
    }
    match context
        .stream_resources_mut()
        .read_line(id, usize::MAX, None, true, true)
    {
        Some(bytes) if !bytes.is_empty() => values.string_bytes_value(&bytes),
        Some(_) => values.bool_value(false),
        None => values.bool_value(false),
    }
}
