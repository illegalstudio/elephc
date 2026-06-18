//! Purpose:
//! Dispatches already evaluated filesystem and path builtins by dynamic callable name.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry::dispatch`.
//!
//! Key details:
//! - Returns `Ok(None)` for names outside this domain so the parent dispatcher can
//!   continue probing other builtin families.

use super::super::super::super::*;
use super::super::super::*;

/// Attempts to dispatch evaluated filesystem and path builtins.
pub(in crate::interpreter) fn eval_filesystem_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "chdir" | "mkdir" | "rmdir" => {
            let [path] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_unary_path_bool_result(name, *path, values)?
        }
        "chmod" => {
            let [filename, permissions] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_chmod_result(*filename, *permissions, values)?
        }
        "chown" | "chgrp" | "lchown" | "lchgrp" => {
            let [filename, principal] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_chown_like_result(name, *filename, *principal, values)?
        }
        "clearstatcache" => {
            if evaluated_args.len() > 2 {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.null()?
        }
        "copy" | "link" | "rename" | "symlink" => {
            let [from, to] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_binary_path_bool_result(name, *from, *to, values)?
        }
        "file" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_result(*filename, values)?
        }
        "file_exists" | "is_dir" | "is_executable" | "is_file" | "is_link" | "is_readable"
        | "is_writable" | "is_writeable" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_probe_result(name, *filename, values)?
        }
        "fileatime" | "filectime" | "filegroup" | "fileinode" | "filemtime" | "fileowner"
        | "fileperms" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_stat_scalar_result(name, *filename, values)?
        }
        "file_get_contents" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_get_contents_result(*filename, values)?
        }
        "file_put_contents" => {
            let [filename, data] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_put_contents_result(*filename, *data, values)?
        }
        "filesize" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_filesize_result(*filename, values)?
        }
        "filetype" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_filetype_result(*filename, values)?
        }
        "fnmatch" => match evaluated_args {
            [pattern, filename] => eval_fnmatch_result(*pattern, *filename, None, values)?,
            [pattern, filename, flags] => {
                eval_fnmatch_result(*pattern, *filename, Some(*flags), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "stat" | "lstat" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stat_array_result(name, *filename, values)?
        }
        "linkinfo" => {
            let [path] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_linkinfo_result(*path, values)?
        }
        "readfile" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_readfile_result(*filename, values)?
        }
        "scandir" => {
            let [directory] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_scandir_result(*directory, values)?
        }
        "basename" => match evaluated_args {
            [path] => eval_basename_result(*path, None, values)?,
            [path, suffix] => eval_basename_result(*path, Some(*suffix), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "dirname" => match evaluated_args {
            [path] => eval_dirname_result(*path, None, values)?,
            [path, levels] => eval_dirname_result(*path, Some(*levels), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "disk_free_space" | "disk_total_space" => {
            let [directory] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_disk_space_result(name, *directory, values)?
        }
        "getcwd" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_getcwd_result(values)?
        }
        "glob" => {
            let [pattern] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_glob_result(*pattern, values)?
        }
        "pathinfo" => match evaluated_args {
            [path] => eval_pathinfo_result(*path, None, values)?,
            [path, flags] => eval_pathinfo_result(*path, Some(*flags), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "realpath" => {
            let [path] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_realpath_result(*path, values)?
        }
        "stream_resolve_include_path" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_resolve_include_path_result(*filename, values)?
        }
        "realpath_cache_get" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_realpath_cache_get_result(values)?
        }
        "realpath_cache_size" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_realpath_cache_size_result(values)?
        }
        "sys_get_temp_dir" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_sys_get_temp_dir_result(values)?
        }
        "tempnam" => {
            let [directory, prefix] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_tempnam_result(*directory, *prefix, values)?
        }
        "touch" => match evaluated_args {
            [filename] => eval_touch_result(*filename, None, None, values)?,
            [filename, mtime] => eval_touch_result(*filename, Some(*mtime), None, values)?,
            [filename, mtime, atime] => {
                eval_touch_result(*filename, Some(*mtime), Some(*atime), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "umask" => match evaluated_args {
            [] => eval_umask_result(None, values)?,
            [mask] => eval_umask_result(Some(*mask), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "readlink" => {
            let [path] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_readlink_result(*path, values)?
        }
        "unlink" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_unlink_result(*filename, values)?
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}
