//! Purpose:
//! Declarative eval registry entry for `readfile`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the streaming file output helper.

eval_builtin! {
    name: "readfile",
    area: Filesystem,
    params: [filename],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;
use crate::stream_wrappers;

/// Dispatches direct eval calls for the `readfile` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_readfile_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_readfile(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `readfile` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_readfile_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename] => eval_readfile_result(*filename, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `readfile($filename)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_readfile(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_readfile_result(filename, context, values)
}

/// Streams one local file or supported wrapper to eval output.
pub(in crate::interpreter) fn eval_readfile_result(
    filename: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if let Some(result) = eval_user_wrapper_readfile_result(&path, context, values)? {
        return Ok(result);
    }
    if let Some(local_path) = stream_wrappers::local_filesystem_path(&path) {
        let path = std::path::Path::new(&local_path);
        if path.is_dir() {
            return values.int(-1);
        }
    }
    let bytes = match super::file_get_contents::eval_read_path_or_wrapper_bytes(&path) {
        Ok(bytes) => bytes,
        Err(_) => return values.bool_value(false),
    };
    let output = values.string_bytes_value(&bytes)?;
    values.echo(output)?;
    values.int(i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?)
}
