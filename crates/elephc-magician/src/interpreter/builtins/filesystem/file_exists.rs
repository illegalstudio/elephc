//! Purpose:
//! Declarative eval registry entry for `file_exists`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the file-probe helper.

eval_builtin! {
    name: "file_exists",
    area: Filesystem,
    params: [filename],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use crate::stream_wrappers;
use super::*;

/// Dispatches direct eval calls for the `file_exists` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_file_exists_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_file_probe("file_exists", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `file_exists` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_file_exists_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename] => eval_file_probe_result("file_exists", *filename, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates one PHP filesystem predicate over an eval expression.
pub(in crate::interpreter) fn eval_builtin_file_probe(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_file_probe_result(name, filename, context, values)
}

/// Computes one local filesystem predicate and returns a PHP boolean.
pub(in crate::interpreter) fn eval_file_probe_result(
    name: &str,
    filename: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if let Some(stat) = eval_user_wrapper_url_stat_result(&path, 0, context, values)? {
        return eval_user_wrapper_file_probe_from_stat(name, stat, values);
    }
    if stream_wrappers::is_phar_stream(&path) {
        let exists = elephc_phar::extract_url_bytes(path.as_bytes()).is_some();
        let supported = matches!(name, "file_exists" | "is_file" | "is_readable");
        return values.bool_value(supported && exists);
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
    let path = std::path::Path::new(&path);
    let result = match name {
        "file_exists" => path.exists(),
        "is_dir" => path.is_dir(),
        "is_executable" => eval_path_is_executable(path),
        "is_file" => path.is_file(),
        "is_link" => std::fs::symlink_metadata(path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false),
        "is_readable" => eval_path_is_readable(path),
        "is_writable" | "is_writeable" => eval_path_is_writable(path),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(result)
}
