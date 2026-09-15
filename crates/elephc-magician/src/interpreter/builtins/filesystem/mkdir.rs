//! Purpose:
//! Declarative eval registry entry for `mkdir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - The one-argument form keeps delegating to the unary path operation helper; the optional
//!   `$permissions` and `$recursive` take their own path, mirroring the AOT runtime (issue #506).
//! - `$permissions` goes through `DirBuilderExt::mode`, which hands the mode to `mkdir(2)` so
//!   the process umask applies exactly as it does in PHP and in the compiled runtime. Setting
//!   the mode after creation instead would bypass the umask and leave a window at the wider
//!   permissions.
//! - Recursive creation builds the PARENTS recursively and the final component on its own, so
//!   an existing target still reports `false`. `create_dir_all` alone succeeds on an existing
//!   directory, which is neither PHP's answer nor the compiled runtime's.

eval_builtin! {
    contract: "mkdir",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::user_wrapper_path_ops::{eval_user_wrapper_mkdir_result, DEFAULT_MKDIR_PERMISSIONS};
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
    if let Some(result) = eval_user_wrapper_mkdir_result(&path, mode, recursive, context, values)? {
        return Ok(result);
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
    // Only the FINAL component decides the result, which is why it is always created on its
    // own: `create_dir_all` succeeds on a directory that already exists, and PHP reports
    // `false` for that.
    if recursive {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                let mut parents = std::fs::DirBuilder::new();
                parents.recursive(true);
                apply_directory_mode(&mut parents, mode);
                if parents.create(parent).is_err() {
                    return values.bool_value(false);
                }
            }
        }
    }
    let mut builder = std::fs::DirBuilder::new();
    apply_directory_mode(&mut builder, mode);
    values.bool_value(builder.create(&path).is_ok())
}

/// Hands PHP's `$permissions` to `mkdir(2)` so the process umask applies, as it does in PHP.
#[cfg(unix)]
fn apply_directory_mode(builder: &mut std::fs::DirBuilder, mode: i64) {
    use std::os::unix::fs::DirBuilderExt;
    let mode = u32::try_from(mode).unwrap_or(DEFAULT_MKDIR_PERMISSIONS as u32);
    builder.mode(mode & 0o7777);
}

/// Non-Unix targets have no POSIX mode to apply.
#[cfg(not(unix))]
fn apply_directory_mode(_builder: &mut std::fs::DirBuilder, _mode: i64) {}
