//! Purpose:
//! Declarative eval registry entry for `chdir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the unary path operation helper.

eval_builtin! {
    contract: "chdir",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use crate::stream_wrappers;
use super::*;

/// Dispatches direct eval calls for the `chdir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_chdir_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_unary_path_bool("chdir", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `chdir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_chdir_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [path] => eval_unary_path_bool_result("chdir", *path, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates a one-path filesystem operation that returns a PHP boolean.
pub(in crate::interpreter) fn eval_builtin_unary_path_bool(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [path] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let path = eval_expr(path, context, scope, values)?;
    eval_unary_path_bool_result(name, path, context, values)
}

/// Executes a one-path filesystem operation and returns whether it succeeded.
pub(in crate::interpreter) fn eval_unary_path_bool_result(
    name: &str,
    path: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_path_bool_result_with_mkdir_options(name, path, MkdirOptions::DEFAULT, context, values)
}

/// `mkdir()`'s `$permissions` and `$recursive`, carried to both the wrapper and the local route.
#[derive(Clone, Copy)]
pub(in crate::interpreter) struct MkdirOptions {
    /// The requested directory mode; php defaults to `0777`.
    pub permissions: u32,
    /// Whether missing parents should be created too.
    pub recursive: bool,
}

impl MkdirOptions {
    /// php's own defaults, and the values every non-`mkdir` caller passes.
    pub(in crate::interpreter) const DEFAULT: Self = Self {
        permissions: 0o777,
        recursive: false,
    };
}

/// Executes a one-path filesystem operation, honouring `mkdir()`'s extra parameters.
pub(in crate::interpreter) fn eval_path_bool_result_with_mkdir_options(
    name: &str,
    path: RuntimeCellHandle,
    options: MkdirOptions,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(path, values)?;
    if let Some(result) =
        eval_user_wrapper_single_path_op_result(name, &path, options, context, values)?
    {
        return Ok(result);
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
    let ok = match name {
        "chdir" => std::env::set_current_dir(path).is_ok(),
        // `create_dir_all` succeeds on an existing directory, but php reports false there, so the
        // parents are created separately and only the leaf decides the answer — the same split the
        // compiled `__rt_mkdir` makes.
        "mkdir" => {
            if options.recursive {
                if let Some(parent) = std::path::Path::new(&path).parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
            }
            create_dir_with_permissions(&path, options.permissions)
        }
        "rmdir" => std::fs::remove_dir(path).is_ok(),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(ok)
}

/// Creates one directory with the requested mode, mirroring php's `mkdir($p, $permissions)`.
fn create_dir_with_permissions(path: &str, permissions: u32) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .mode(permissions)
            .create(path)
            .is_ok()
    }
    #[cfg(not(unix))]
    {
        let _ = permissions;
        std::fs::create_dir(path).is_ok()
    }
}
