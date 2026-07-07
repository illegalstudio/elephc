//! Purpose:
//! Declarative eval registry entries and dispatch adapters for filesystem builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem` module loading.
//! - `crate::interpreter::builtins::hooks` for migrated filesystem dispatch.
//!
//! Key details:
//! - This covers simple path/query helpers; stream, stat, mutating, and
//!   by-reference filesystem calls stay on the legacy path.

use super::super::super::*;
use super::*;

mod basename;
mod dirname;
mod disk_free_space;
mod disk_total_space;
mod fnmatch;
mod getcwd;
mod glob;
mod linkinfo;
mod pathinfo;
mod readlink;
mod realpath;
mod realpath_cache_get;
mod realpath_cache_size;
mod stream_resolve_include_path;
mod sys_get_temp_dir;

/// Dispatches direct expression-level calls for declaratively migrated filesystem builtins.
pub(in crate::interpreter) fn eval_builtin_filesystem_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "basename" => eval_builtin_basename(args, context, scope, values),
        "dirname" => eval_builtin_dirname(args, context, scope, values),
        "disk_free_space" | "disk_total_space" => {
            eval_builtin_disk_space(name, args, context, scope, values)
        }
        "fnmatch" => eval_builtin_fnmatch(args, context, scope, values),
        "getcwd" => eval_builtin_getcwd(args, values),
        "glob" => eval_builtin_glob(args, context, scope, values),
        "linkinfo" => eval_builtin_linkinfo(args, context, scope, values),
        "pathinfo" => eval_builtin_pathinfo(args, context, scope, values),
        "readlink" => eval_builtin_readlink(args, context, scope, values),
        "realpath" => eval_builtin_realpath(args, context, scope, values),
        "realpath_cache_get" => eval_builtin_realpath_cache_get(args, values),
        "realpath_cache_size" => eval_builtin_realpath_cache_size(args, values),
        "stream_resolve_include_path" => {
            eval_builtin_stream_resolve_include_path(args, context, scope, values)
        }
        "sys_get_temp_dir" => eval_builtin_sys_get_temp_dir(args, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches evaluated-argument calls for declaratively migrated filesystem builtins.
pub(in crate::interpreter) fn eval_filesystem_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "basename" => match evaluated_args {
            [path] => eval_basename_result(*path, None, values),
            [path, suffix] => eval_basename_result(*path, Some(*suffix), values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "dirname" => match evaluated_args {
            [path] => eval_dirname_result(*path, None, values),
            [path, levels] => eval_dirname_result(*path, Some(*levels), values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "disk_free_space" | "disk_total_space" => match evaluated_args {
            [directory] => eval_disk_space_result(name, *directory, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "fnmatch" => match evaluated_args {
            [pattern, filename] => eval_fnmatch_result(*pattern, *filename, None, values),
            [pattern, filename, flags] => {
                eval_fnmatch_result(*pattern, *filename, Some(*flags), values)
            }
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "getcwd" => match evaluated_args {
            [] => eval_getcwd_result(values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "glob" => match evaluated_args {
            [pattern] => eval_glob_result(*pattern, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "linkinfo" => match evaluated_args {
            [path] => eval_linkinfo_result(*path, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "pathinfo" => match evaluated_args {
            [path] => eval_pathinfo_result(*path, None, values),
            [path, flags] => eval_pathinfo_result(*path, Some(*flags), values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "readlink" => match evaluated_args {
            [path] => eval_readlink_result(*path, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "realpath" => match evaluated_args {
            [path] => eval_realpath_result(*path, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "realpath_cache_get" => match evaluated_args {
            [] => eval_realpath_cache_get_result(values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "realpath_cache_size" => match evaluated_args {
            [] => eval_realpath_cache_size_result(values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "stream_resolve_include_path" => match evaluated_args {
            [filename] => eval_stream_resolve_include_path_result(*filename, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "sys_get_temp_dir" => match evaluated_args {
            [] => eval_sys_get_temp_dir_result(values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
