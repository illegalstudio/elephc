//! Purpose:
//! Declarative eval registry entry for `opendir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the directory resource open helper.

eval_builtin! {
    name: "opendir",
    area: Filesystem,
    params: [directory],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `opendir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_opendir_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_opendir(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `opendir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_opendir_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [directory] => eval_opendir_result(*directory, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `opendir($directory)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_opendir(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [directory] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let directory = eval_expr(directory, context, scope, values)?;
    eval_opendir_result(directory, context, values)
}

/// Opens a local directory and returns a resource cell or PHP false.
pub(in crate::interpreter) fn eval_opendir_result(
    directory: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let directory = eval_path_string(directory, values)?;
    if let Some(result) = eval_user_wrapper_opendir_result(&directory, context, values)? {
        return Ok(result);
    }
    match context.stream_resources_mut().open_directory(&directory) {
        Some(id) => values.resource(id),
        None => values.bool_value(false),
    }
}
