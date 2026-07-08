//! Purpose:
//! Evaluated-argument dispatch for declarative path, file, directory, and stat builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::values_dispatch`.
//!
//! Key details:
//! - This module owns by-value dispatch for filesystem helpers that do not work
//!   with stream resource cursor state.

use super::super::super::*;
use super::*;

/// Attempts evaluated-argument dispatch for path and file builtins.
pub(in crate::interpreter::builtins::filesystem) fn eval_filesystem_path_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "file_exists" | "is_dir" | "is_executable" | "is_file" | "is_link" | "is_readable"
        | "is_writable" | "is_writeable" => match evaluated_args {
            [filename] => eval_file_probe_result(name, *filename, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "fileatime" | "filectime" | "filegroup" | "fileinode" | "filemtime" | "fileowner"
        | "fileperms" => match evaluated_args {
            [filename] => eval_file_stat_scalar_result(name, *filename, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "filesize" => match evaluated_args {
            [filename] => eval_filesize_result(*filename, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "filetype" => match evaluated_args {
            [filename] => eval_filetype_result(*filename, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "fnmatch" => match evaluated_args {
            [pattern, filename] => eval_fnmatch_result(*pattern, *filename, None, values)?,
            [pattern, filename, flags] => {
                eval_fnmatch_result(*pattern, *filename, Some(*flags), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "getcwd" => match evaluated_args {
            [] => eval_getcwd_result(values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "opendir" => match evaluated_args {
            [directory] => eval_opendir_result(*directory, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "pclose" => match evaluated_args {
            [handle] => eval_pclose_result(*handle, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "popen" => match evaluated_args {
            [command, mode] => eval_popen_result(*command, *mode, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "closedir" | "readdir" | "rewinddir" => match evaluated_args {
            [dir_handle] => eval_unary_directory_result(name, *dir_handle, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "realpath_cache_get" => match evaluated_args {
            [] => eval_realpath_cache_get_result(values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "realpath_cache_size" => match evaluated_args {
            [] => eval_realpath_cache_size_result(values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "stat" | "lstat" => match evaluated_args {
            [filename] => eval_stat_array_result(name, *filename, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "sys_get_temp_dir" => match evaluated_args {
            [] => eval_sys_get_temp_dir_result(values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "tmpfile" => match evaluated_args {
            [] => eval_tmpfile_result(context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        _ => return Ok(None),
    };
    Ok(Some(result))
}
