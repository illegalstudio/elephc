//! Purpose:
//! Declarative eval registry entry for `mkdir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - The one-argument form keeps delegating to the unary path operation helper; the optional
//!   `$permissions` and `$recursive` take their own path, mirroring the AOT runtime (issue #506).
//! - `$permissions` is applied after creation rather than through `create_dir`, which has no
//!   mode argument in `std`.

eval_builtin! {
    contract: "mkdir",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use crate::stream_wrappers;

/// Dispatches direct eval calls for the `mkdir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_mkdir_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (path, permissions, recursive) = match args {
        [path] => {
            let _ = path;
            return super::chdir::eval_builtin_unary_path_bool(
                "mkdir", args, context, scope, values,
            );
        }
        [path, permissions] => (path, permissions, None),
        [path, permissions, recursive] => (path, permissions, Some(recursive)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let path = eval_expr(path, context, scope, values)?;
    let permissions = eval_expr(permissions, context, scope, values)?;
    let mode = eval_int_value(permissions, values)?;
    let recursive = match recursive {
        Some(recursive) => {
            let recursive = eval_expr(recursive, context, scope, values)?;
            eval_int_value(recursive, values)? != 0
        }
        None => false,
    };
    eval_mkdir_with_options(path, mode, recursive, context, values)
}

/// Dispatches evaluated-argument calls for the `mkdir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_mkdir_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [path] => super::chdir::eval_unary_path_bool_result("mkdir", *path, context, values),
        [path, permissions] => {
            let mode = eval_int_value(*permissions, values)?;
            eval_mkdir_with_options(*path, mode, false, context, values)
        }
        [path, permissions, recursive] => {
            let mode = eval_int_value(*permissions, values)?;
            let recursive = eval_int_value(*recursive, values)? != 0;
            eval_mkdir_with_options(*path, mode, recursive, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Creates one directory, optionally with its parents, and applies the requested mode.
fn eval_mkdir_with_options(
    path: RuntimeCellHandle,
    mode: i64,
    recursive: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(path, values)?;
    if let Some(result) = eval_user_wrapper_single_path_op_result("mkdir", &path, context, values)? {
        return Ok(result);
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
    let created = if recursive {
        std::fs::create_dir_all(&path)
    } else {
        std::fs::create_dir(&path)
    };
    if created.is_err() {
        return values.bool_value(false);
    }
    apply_directory_mode(&path, mode);
    values.bool_value(true)
}

/// Applies PHP's `$permissions` to a directory that was just created.
#[cfg(unix)]
fn apply_directory_mode(path: &str, mode: i64) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(mode) = u32::try_from(mode) else {
        return;
    };
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & 0o7777));
}

/// Non-Unix targets have no POSIX mode to apply.
#[cfg(not(unix))]
fn apply_directory_mode(_path: &str, _mode: i64) {}
