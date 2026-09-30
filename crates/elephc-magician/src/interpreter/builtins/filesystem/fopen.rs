//! Purpose:
//! Declarative eval registry entry for `fopen`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the stream-opening helper.

eval_builtin! {
    contract: "fopen",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `fopen` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fopen_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_fopen(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fopen` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fopen_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&evaluated_args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_fopen_result(evaluated_args[0], evaluated_args[1], context, values)
}

/// Evaluates PHP `fopen($filename, $mode, ...)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_fopen(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let filename = eval_expr(&args[0], context, scope, values)?;
    let mode = eval_expr(&args[1], context, scope, values)?;
    for arg in &args[2..] {
        eval_expr(arg, context, scope, values)?;
    }
    let filename = eval_path_string(filename, values)?;
    let mode = eval_stream_string(mode, values)?;
    eval_fopen_path_result(&filename, &mode, context, scope, values)
}

/// Opens a local file stream and returns a resource cell or PHP false.
pub(in crate::interpreter) fn eval_fopen_result(
    filename: RuntimeCellHandle,
    mode: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let filename = eval_path_string(filename, values)?;
    let mode = eval_stream_string(mode, values)?;
    let mut scope = ElephcEvalScope::new();
    eval_fopen_path_result(&filename, &mode, context, &mut scope, values)
}

/// Opens a stream by already-coerced path and mode strings.
fn eval_fopen_path_result(
    filename: &str,
    mode: &str,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) = eval_user_wrapper_fopen_result(filename, mode, context, scope, values)? {
        return Ok(result);
    }
    match context.stream_resources_mut().open_path(filename, mode) {
        Some(id) => values.resource(id),
        None => {
            let reason = context.stream_resources_mut().last_open_reason();
            values.warning(&format!(
                "Warning: fopen({filename}): Failed to open stream: {reason}\n"
            ))?;
            values.bool_value(false)
        }
    }
}
