//! Purpose:
//! Direct expression-level dispatch for filesystem builtins declared in the eval registry.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks::EvalDirectHook::call()`.
//!
//! Key details:
//! - This dispatcher keeps filesystem call lowering area-scoped while per-builtin
//!   metadata lives in individual declaration files.

use super::super::super::super::*;
use super::super::*;

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
        "fgetcsv" => eval_builtin_fgetcsv(args, context, scope, values),
        "file" => eval_builtin_file(args, context, scope, values),
        "file_exists" | "is_dir" | "is_executable" | "is_file" | "is_link" | "is_readable"
        | "is_writable" | "is_writeable" => {
            eval_builtin_file_probe(name, args, context, scope, values)
        }
        "file_get_contents" => eval_builtin_file_get_contents(args, context, scope, values),
        "file_put_contents" => eval_builtin_file_put_contents(args, context, scope, values),
        "fileatime" | "filectime" | "filegroup" | "fileinode" | "filemtime" | "fileowner"
        | "fileperms" => eval_builtin_file_stat_scalar(name, args, context, scope, values),
        "filesize" => eval_builtin_filesize(args, context, scope, values),
        "filetype" => eval_builtin_filetype(args, context, scope, values),
        "fclose" | "fgetc" | "fgets" | "feof" | "fflush" | "fpassthru" | "fsync"
        | "fdatasync" | "ftell" | "rewind" | "fstat" | "stream_get_meta_data" => {
            eval_builtin_unary_stream(name, args, context, scope, values)
        }
        "fnmatch" => eval_builtin_fnmatch(args, context, scope, values),
        "fopen" => eval_builtin_fopen(args, context, scope, values),
        "fprintf" => eval_builtin_fprintf(args, context, scope, values),
        "fputcsv" => eval_builtin_fputcsv(args, context, scope, values),
        "fread" => eval_builtin_fread(args, context, scope, values),
        "fscanf" => eval_builtin_fscanf(args, context, scope, values),
        "fseek" => eval_builtin_fseek(args, context, scope, values),
        "ftruncate" => eval_builtin_ftruncate(args, context, scope, values),
        "fwrite" => eval_builtin_fwrite(args, context, scope, values),
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
        "readline" => eval_builtin_readline(args, context, scope, values),
        "readlink" => eval_builtin_readlink(args, context, scope, values),
        "realpath" => eval_builtin_realpath(args, context, scope, values),
        "realpath_cache_get" => eval_builtin_realpath_cache_get(args, values),
        "realpath_cache_size" => eval_builtin_realpath_cache_size(args, values),
        "scandir" => eval_builtin_scandir(args, context, scope, values),
        "stat" | "lstat" => eval_builtin_stat_array(name, args, context, scope, values),
        "stream_bucket_append" | "stream_bucket_prepend" => {
            eval_builtin_stream_bucket_push(name, args, context, scope, values)
        }
        "stream_bucket_make_writeable" => {
            eval_builtin_stream_bucket_make_writeable(args, context, scope, values)
        }
        "stream_bucket_new" => eval_builtin_stream_bucket_new(args, context, scope, values),
        "stream_context_create" => eval_builtin_stream_context_create(args, context, scope, values),
        "stream_context_get_default" => {
            eval_builtin_stream_context_get_default(args, context, scope, values)
        }
        "stream_context_get_options" => {
            eval_builtin_stream_context_get_options(args, context, scope, values)
        }
        "stream_context_get_params" => {
            eval_builtin_stream_context_get_params(args, context, scope, values)
        }
        "stream_context_set_default" => {
            eval_builtin_stream_context_set_default(args, context, scope, values)
        }
        "stream_context_set_option" => {
            eval_builtin_stream_context_set_option(args, context, scope, values)
        }
        "stream_context_set_params" => {
            eval_builtin_stream_context_set_params(args, context, scope, values)
        }
        "stream_copy_to_stream" => eval_builtin_stream_copy_to_stream(args, context, scope, values),
        "stream_filter_append" | "stream_filter_prepend" => {
            eval_builtin_stream_filter_attach(name, args, context, scope, values)
        }
        "stream_filter_register" => {
            eval_builtin_stream_filter_register(args, context, scope, values)
        }
        "stream_filter_remove" => eval_builtin_stream_filter_remove(args, context, scope, values),
        "stream_get_contents" => eval_builtin_stream_get_contents(args, context, scope, values),
        "stream_get_line" => eval_builtin_stream_get_line(args, context, scope, values),
        "stream_isatty" => eval_builtin_stream_isatty(args, context, scope, values),
        "stream_resolve_include_path" => {
            eval_builtin_stream_resolve_include_path(args, context, scope, values)
        }
        "stream_set_blocking" => eval_builtin_stream_set_blocking(args, context, scope, values),
        "stream_set_chunk_size" | "stream_set_read_buffer" | "stream_set_write_buffer" => {
            eval_builtin_stream_set_buffer_like(name, args, context, scope, values)
        }
        "stream_set_timeout" => eval_builtin_stream_set_timeout(args, context, scope, values),
        "stream_socket_client" => eval_builtin_stream_socket_client(args, context, scope, values),
        "stream_socket_enable_crypto" => {
            eval_builtin_stream_socket_enable_crypto(args, context, scope, values)
        }
        "stream_socket_get_name" => {
            eval_builtin_stream_socket_get_name(args, context, scope, values)
        }
        "stream_socket_pair" => eval_builtin_stream_socket_pair(args, context, scope, values),
        "stream_socket_sendto" => eval_builtin_stream_socket_sendto(args, context, scope, values),
        "stream_socket_server" => eval_builtin_stream_socket_server(args, context, scope, values),
        "stream_socket_shutdown" => {
            eval_builtin_stream_socket_shutdown(args, context, scope, values)
        }
        "stream_wrapper_register" | "stream_wrapper_unregister" | "stream_wrapper_restore" => {
            eval_builtin_stream_wrapper_registry(name, args, context, scope, values)
        }
        "sys_get_temp_dir" => eval_builtin_sys_get_temp_dir(args, values),
        "tempnam" => eval_builtin_tempnam(args, context, scope, values),
        "tmpfile" => eval_builtin_tmpfile(args, context, values),
        "touch" => eval_builtin_touch(args, context, scope, values),
        "umask" => eval_builtin_umask(args, context, scope, values),
        "unlink" => eval_builtin_unlink(args, context, scope, values),
        "vfprintf" => eval_builtin_vfprintf(args, context, scope, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
