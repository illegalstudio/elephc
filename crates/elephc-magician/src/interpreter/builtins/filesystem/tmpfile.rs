//! Purpose:
//! Declarative eval registry entry for `tmpfile`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the temporary stream helper.

eval_builtin! {
    name: "tmpfile",
    area: Filesystem,
    params: [],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `tmpfile` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_tmpfile_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_tmpfile(args, context, values)
}

/// Dispatches evaluated-argument calls for the `tmpfile` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_tmpfile_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [] => eval_tmpfile_result(context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `tmpfile()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_tmpfile(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_tmpfile_result(context, values)
}

/// Creates an anonymous temporary file stream resource or returns PHP false.
pub(in crate::interpreter) fn eval_tmpfile_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match context.stream_resources_mut().open_tmpfile() {
        Some(id) => values.resource(id),
        None => values.bool_value(false),
    }
}
