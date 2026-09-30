//! Purpose:
//! Declarative eval registry entry for `fscanf`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - The eval implementation returns the parsed values as an array and REFUSES the by-ref
//!   `$vars` output form, matching the compiled builtin. php assigns each field through the
//!   reference and returns the field COUNT; ignoring the output vars — as this file used to —
//!   made the call silently return the array and assign nothing, which also let `eval()` serve
//!   as a silent-wrong workaround for the compiled path's refusal.

eval_builtin! {
    contract: "fscanf",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `fscanf` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fscanf_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_fscanf(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fscanf` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fscanf_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // Exactly two: a bound `$vars` tail is the unsupported by-ref output form.
    if evaluated_args.len() != 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_fscanf_result(evaluated_args[0], evaluated_args[1], context, values)
}

/// Evaluates PHP `fscanf($stream, $format, ...$vars)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_fscanf(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // Exactly two: a trailing `$vars` list is the unsupported by-ref output form.
    if args.len() != 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let format = eval_expr(&args[1], context, scope, values)?;
    eval_fscanf_result(stream, format, context, values)
}

/// Reads one line from a stream and scans it with the eval `sscanf()` subset.
pub(in crate::interpreter) fn eval_fscanf_result(
    stream: RuntimeCellHandle,
    format: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let Some(line) = context
        .stream_resources_mut()
        .read_line(id, usize::MAX, None, true, true)
    else {
        return values.bool_value(false);
    };
    let input = values.string_bytes_value(&line)?;
    eval_sscanf_result(input, format, context, values)
}
