//! Purpose:
//! Declarative eval registry entry for `copy`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the binary path operation helper.

eval_builtin! {
    name: "copy",
    area: Filesystem,
    params: [from, to],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use crate::stream_wrappers;
use super::*;

/// Dispatches direct eval calls for the `copy` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_copy_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_binary_path_bool("copy", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `copy` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_copy_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [from, to] => eval_binary_path_bool_result("copy", *from, *to, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates a two-path filesystem operation that returns a PHP boolean.
pub(in crate::interpreter) fn eval_builtin_binary_path_bool(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [from, to] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let from = eval_expr(from, context, scope, values)?;
    let to = eval_expr(to, context, scope, values)?;
    eval_binary_path_bool_result(name, from, to, context, values)
}

/// Executes a two-path filesystem operation and returns whether it succeeded.
pub(in crate::interpreter) fn eval_binary_path_bool_result(
    name: &str,
    from: RuntimeCellHandle,
    to: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let from = eval_path_string(from, values)?;
    let to = eval_path_string(to, values)?;
    if name == "rename" {
        if let Some(result) = eval_user_wrapper_rename_result(&from, &to, context, values)? {
            return Ok(result);
        }
    }
    let Some(from) = stream_wrappers::local_filesystem_path(&from) else {
        return values.bool_value(false);
    };
    let Some(to) = stream_wrappers::local_filesystem_path(&to) else {
        return values.bool_value(false);
    };
    #[cfg(windows)]
    let source_mode = context.capture_local_file_mode(&from);
    #[cfg(windows)]
    let destination_mode = context.capture_local_file_mode(&to);
    let ok = match name {
        "copy" => std::fs::copy(&from, &to).is_ok(),
        "link" => std::fs::hard_link(&from, &to).is_ok(),
        "rename" => std::fs::rename(&from, &to).is_ok(),
        "symlink" => eval_create_symlink(&from, &to),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    #[cfg(windows)]
    if ok {
        match name {
            "copy" | "link" => context.copy_local_file_mode(
                source_mode.as_ref(),
                destination_mode.as_ref(),
                &to,
            ),
            "rename" => context.rename_local_file_mode(source_mode, destination_mode, &to),
            "symlink" => {}
            _ => unreachable!("filesystem operation was validated above"),
        }
    }
    values.bool_value(ok)
}

/// Creates a symbolic link with the host-specific path API.
fn eval_create_symlink(from: &str, to: &str) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(from, to).is_ok()
    }
    #[cfg(windows)]
    {
        if std::fs::metadata(from).is_ok_and(|metadata| metadata.is_dir()) {
            std::os::windows::fs::symlink_dir(from, to).is_ok()
        } else {
            std::os::windows::fs::symlink_file(from, to).is_ok()
        }
    }
}
