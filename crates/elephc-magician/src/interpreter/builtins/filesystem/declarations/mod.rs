//! Purpose:
//! Declarative eval registry entries and dispatch adapters for filesystem builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem` module loading.
//! - `crate::interpreter::builtins::hooks` for migrated filesystem dispatch.
//!
//! Key details:
//! - This covers simple path/query/content and non-by-ref mutation helpers;
//!   stream and by-reference filesystem calls stay on the legacy path.

use super::super::super::*;
use super::*;

mod basename;
mod chdir;
mod chgrp;
mod chmod;
mod chown;
mod closedir;
mod clearstatcache;
mod copy;
mod dirname;
mod disk_free_space;
mod disk_total_space;
mod file;
mod file_exists;
mod file_get_contents;
mod file_put_contents;
mod fileatime;
mod filectime;
mod filegroup;
mod fileinode;
mod filemtime;
mod fileowner;
mod fileperms;
mod filesize;
mod filetype;
mod fnmatch;
mod getcwd;
mod glob;
mod is_dir;
mod is_executable;
mod is_file;
mod is_link;
mod is_readable;
mod is_writable;
mod is_writeable;
mod lchgrp;
mod lchown;
mod link;
mod linkinfo;
mod lstat;
mod mkdir;
mod opendir;
mod pathinfo;
mod pclose;
mod popen;
mod readdir;
mod readfile;
mod readlink;
mod realpath;
mod realpath_cache_get;
mod realpath_cache_size;
mod rename;
mod rewinddir;
mod rmdir;
mod scandir;
mod stat;
mod stream_resolve_include_path;
mod symlink;
mod sys_get_temp_dir;
mod tempnam;
mod tmpfile;
mod touch;
mod umask;
mod unlink;

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
        "chdir" | "mkdir" | "rmdir" => {
            eval_builtin_unary_path_bool(name, args, context, scope, values)
        }
        "chmod" => eval_builtin_chmod(args, context, scope, values),
        "chown" | "chgrp" | "lchown" | "lchgrp" => {
            eval_builtin_chown_like(name, args, context, scope, values)
        }
        "clearstatcache" => eval_builtin_clearstatcache(args, context, scope, values),
        "copy" | "link" | "rename" | "symlink" => {
            eval_builtin_binary_path_bool(name, args, context, scope, values)
        }
        "dirname" => eval_builtin_dirname(args, context, scope, values),
        "disk_free_space" | "disk_total_space" => {
            eval_builtin_disk_space(name, args, context, scope, values)
        }
        "file_exists" | "is_dir" | "is_executable" | "is_file" | "is_link" | "is_readable"
        | "is_writable" | "is_writeable" => {
            eval_builtin_file_probe(name, args, context, scope, values)
        }
        "fileatime" | "filectime" | "filegroup" | "fileinode" | "filemtime" | "fileowner"
        | "fileperms" => eval_builtin_file_stat_scalar(name, args, context, scope, values),
        "file" => eval_builtin_file(args, context, scope, values),
        "file_get_contents" => eval_builtin_file_get_contents(args, context, scope, values),
        "file_put_contents" => eval_builtin_file_put_contents(args, context, scope, values),
        "filesize" => eval_builtin_filesize(args, context, scope, values),
        "filetype" => eval_builtin_filetype(args, context, scope, values),
        "fnmatch" => eval_builtin_fnmatch(args, context, scope, values),
        "getcwd" => eval_builtin_getcwd(args, values),
        "glob" => eval_builtin_glob(args, context, scope, values),
        "linkinfo" => eval_builtin_linkinfo(args, context, scope, values),
        "opendir" => eval_builtin_opendir(args, context, scope, values),
        "pathinfo" => eval_builtin_pathinfo(args, context, scope, values),
        "pclose" => eval_builtin_pclose(args, context, scope, values),
        "popen" => eval_builtin_popen(args, context, scope, values),
        "closedir" | "readdir" | "rewinddir" => {
            eval_builtin_unary_directory(name, args, context, scope, values)
        }
        "readfile" => eval_builtin_readfile(args, context, scope, values),
        "readlink" => eval_builtin_readlink(args, context, scope, values),
        "realpath" => eval_builtin_realpath(args, context, scope, values),
        "realpath_cache_get" => eval_builtin_realpath_cache_get(args, values),
        "realpath_cache_size" => eval_builtin_realpath_cache_size(args, values),
        "scandir" => eval_builtin_scandir(args, context, scope, values),
        "stat" | "lstat" => eval_builtin_stat_array(name, args, context, scope, values),
        "stream_resolve_include_path" => {
            eval_builtin_stream_resolve_include_path(args, context, scope, values)
        }
        "sys_get_temp_dir" => eval_builtin_sys_get_temp_dir(args, values),
        "tempnam" => eval_builtin_tempnam(args, context, scope, values),
        "tmpfile" => eval_builtin_tmpfile(args, context, values),
        "touch" => eval_builtin_touch(args, context, scope, values),
        "umask" => eval_builtin_umask(args, context, scope, values),
        "unlink" => eval_builtin_unlink(args, context, scope, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches evaluated-argument calls for declaratively migrated filesystem builtins.
pub(in crate::interpreter) fn eval_filesystem_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "basename" => match evaluated_args {
            [path] => eval_basename_result(*path, None, values),
            [path, suffix] => eval_basename_result(*path, Some(*suffix), values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "chdir" | "mkdir" | "rmdir" => match evaluated_args {
            [path] => eval_unary_path_bool_result(name, *path, context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "chmod" => match evaluated_args {
            [filename, permissions] => eval_chmod_result(*filename, *permissions, context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "chown" | "chgrp" | "lchown" | "lchgrp" => match evaluated_args {
            [filename, principal] => {
                eval_chown_like_result(name, *filename, *principal, context, values)
            }
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "clearstatcache" => {
            if evaluated_args.len() > 2 {
                Err(EvalStatus::RuntimeFatal)
            } else {
                values.null()
            }
        }
        "copy" | "link" | "rename" | "symlink" => match evaluated_args {
            [from, to] => eval_binary_path_bool_result(name, *from, *to, context, values),
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
        "file_exists" | "is_dir" | "is_executable" | "is_file" | "is_link" | "is_readable"
        | "is_writable" | "is_writeable" => match evaluated_args {
            [filename] => eval_file_probe_result(name, *filename, context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "fileatime" | "filectime" | "filegroup" | "fileinode" | "filemtime" | "fileowner"
        | "fileperms" => match evaluated_args {
            [filename] => eval_file_stat_scalar_result(name, *filename, context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "file" => match evaluated_args {
            [filename] => eval_file_result(*filename, context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "file_get_contents" => match evaluated_args {
            [filename] => eval_file_get_contents_result(*filename, context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "file_put_contents" => match evaluated_args {
            [filename, data] => eval_file_put_contents_result(*filename, *data, context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "filesize" => match evaluated_args {
            [filename] => eval_filesize_result(*filename, context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "filetype" => match evaluated_args {
            [filename] => eval_filetype_result(*filename, context, values),
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
        "opendir" => match evaluated_args {
            [directory] => eval_opendir_result(*directory, context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "pathinfo" => match evaluated_args {
            [path] => eval_pathinfo_result(*path, None, values),
            [path, flags] => eval_pathinfo_result(*path, Some(*flags), values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "pclose" => match evaluated_args {
            [handle] => eval_pclose_result(*handle, context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "popen" => match evaluated_args {
            [command, mode] => eval_popen_result(*command, *mode, context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "closedir" | "readdir" | "rewinddir" => match evaluated_args {
            [dir_handle] => eval_unary_directory_result(name, *dir_handle, context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "readfile" => match evaluated_args {
            [filename] => eval_readfile_result(*filename, context, values),
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
        "scandir" => match evaluated_args {
            [directory] => eval_scandir_result(*directory, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "stat" | "lstat" => match evaluated_args {
            [filename] => eval_stat_array_result(name, *filename, context, values),
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
        "tempnam" => match evaluated_args {
            [directory, prefix] => eval_tempnam_result(*directory, *prefix, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "tmpfile" => match evaluated_args {
            [] => eval_tmpfile_result(context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "touch" => match evaluated_args {
            [filename] => eval_touch_result(*filename, None, None, context, values),
            [filename, mtime] => eval_touch_result(*filename, Some(*mtime), None, context, values),
            [filename, mtime, atime] => {
                eval_touch_result(*filename, Some(*mtime), Some(*atime), context, values)
            }
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "umask" => match evaluated_args {
            [] => eval_umask_result(None, values),
            [mask] => eval_umask_result(Some(*mask), values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "unlink" => match evaluated_args {
            [filename] => eval_unlink_result(*filename, context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
