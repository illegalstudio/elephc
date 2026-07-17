//! Purpose:
//! Declarative eval registry entry for `fseek`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the stream seek helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "fseek",
    area: Filesystem,
    params: [stream, offset, whence = EvalBuiltinDefaultValue::Int(0)],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `fseek` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fseek_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_fseek(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fseek` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fseek_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream, offset] => eval_fseek_result(*stream, *offset, None, context, values),
        [stream, offset, whence] => eval_fseek_result(*stream, *offset, Some(*whence), context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `fseek($stream, $offset, $whence = SEEK_SET)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_fseek(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=3).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let offset = eval_expr(&args[1], context, scope, values)?;
    let whence = match args.get(2) {
        Some(whence) => Some(eval_expr(whence, context, scope, values)?),
        None => None,
    };
    eval_fseek_result(stream, offset, whence, context, values)
}

/// Seeks a materialized stream and returns PHP's 0 or -1 status code.
pub(in crate::interpreter) fn eval_fseek_result(
    stream: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    whence: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let offset = eval_int_value(offset, values)?;
    let whence = match whence {
        Some(whence) => eval_int_value(whence, values)?,
        None => 0,
    };
    if let Some(seek_ok) = eval_user_wrapper_fseek_result(id, offset, whence, context, values)? {
        return values.int(if seek_ok { 0 } else { -1 });
    }
    let status = if context.stream_resources_mut().seek(id, offset, whence) {
        0
    } else {
        -1
    };
    values.int(status)
}
