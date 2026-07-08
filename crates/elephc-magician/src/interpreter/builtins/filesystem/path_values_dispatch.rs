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
        "basename" => match evaluated_args {
            [path] => eval_basename_result(*path, None, values)?,
            [path, suffix] => eval_basename_result(*path, Some(*suffix), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "chdir" | "mkdir" | "rmdir" => match evaluated_args {
            [path] => eval_unary_path_bool_result(name, *path, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "chmod" => match evaluated_args {
            [filename, permissions] => eval_chmod_result(*filename, *permissions, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "chown" | "chgrp" | "lchown" | "lchgrp" => match evaluated_args {
            [filename, principal] => {
                eval_chown_like_result(name, *filename, *principal, context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "clearstatcache" => {
            if evaluated_args.len() > 2 {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.null()?
        }
        "copy" | "link" | "rename" | "symlink" => match evaluated_args {
            [from, to] => eval_binary_path_bool_result(name, *from, *to, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "dirname" => match evaluated_args {
            [path] => eval_dirname_result(*path, None, values)?,
            [path, levels] => eval_dirname_result(*path, Some(*levels), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "disk_free_space" | "disk_total_space" => match evaluated_args {
            [directory] => eval_disk_space_result(name, *directory, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "file" => match evaluated_args {
            [filename] => eval_file_result(*filename, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "file_exists" | "is_dir" | "is_executable" | "is_file" | "is_link" | "is_readable"
        | "is_writable" | "is_writeable" => match evaluated_args {
            [filename] => eval_file_probe_result(name, *filename, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "file_get_contents" => match evaluated_args {
            [filename] => eval_file_get_contents_result(*filename, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "file_put_contents" => match evaluated_args {
            [filename, data] => {
                eval_file_put_contents_result(*filename, *data, context, values)?
            }
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
        "glob" => match evaluated_args {
            [pattern] => eval_glob_result(*pattern, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "linkinfo" => match evaluated_args {
            [path] => eval_linkinfo_result(*path, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "opendir" => match evaluated_args {
            [directory] => eval_opendir_result(*directory, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "pathinfo" => match evaluated_args {
            [path] => eval_pathinfo_result(*path, None, values)?,
            [path, flags] => eval_pathinfo_result(*path, Some(*flags), values)?,
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
        "readfile" => match evaluated_args {
            [filename] => eval_readfile_result(*filename, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "readlink" => match evaluated_args {
            [path] => eval_readlink_result(*path, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "realpath" => match evaluated_args {
            [path] => eval_realpath_result(*path, values)?,
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
        "scandir" => match evaluated_args {
            [directory] => eval_scandir_result(*directory, values)?,
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
        "tempnam" => match evaluated_args {
            [directory, prefix] => eval_tempnam_result(*directory, *prefix, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "tmpfile" => match evaluated_args {
            [] => eval_tmpfile_result(context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "touch" => match evaluated_args {
            [filename] => eval_touch_result(*filename, None, None, context, values)?,
            [filename, mtime] => {
                eval_touch_result(*filename, Some(*mtime), None, context, values)?
            }
            [filename, mtime, atime] => {
                eval_touch_result(*filename, Some(*mtime), Some(*atime), context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "umask" => match evaluated_args {
            [] => eval_umask_result(None, values)?,
            [mask] => eval_umask_result(Some(*mask), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "unlink" => match evaluated_args {
            [filename] => eval_unlink_result(*filename, context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        _ => return Ok(None),
    };
    Ok(Some(result))
}
