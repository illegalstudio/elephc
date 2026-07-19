//! Purpose:
//! Declarative eval registry entry for `chmod`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the chmod helper.

eval_builtin! {
    name: "chmod",
    area: Filesystem,
    params: [filename, permissions],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use crate::stream_wrappers;
use super::*;

/// Dispatches direct eval calls for the `chmod` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_chmod_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_chmod(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `chmod` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_chmod_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename, permissions] => eval_chmod_result(*filename, *permissions, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `chmod($filename, $permissions)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_chmod(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename, permissions] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    let permissions = eval_expr(permissions, context, scope, values)?;
    eval_chmod_result(filename, permissions, context, values)
}

/// Changes one local file's mode and returns whether the operation succeeded.
pub(in crate::interpreter) fn eval_chmod_result(
    filename: RuntimeCellHandle,
    permissions: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let mode = eval_int_value(permissions, values)? as u32;
    let metadata_value = values.int(i64::from(mode))?;
    if let Some(result) = eval_user_wrapper_stream_metadata_result(
        &path,
        EVAL_STREAM_META_ACCESS,
        metadata_value,
        context,
        values,
    )? {
        return Ok(result);
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
    let permissions = std::fs::Permissions::from_mode(mode);
    values.bool_value(std::fs::set_permissions(path, permissions).is_ok())
}
