//! Purpose:
//! Direct expression-level dispatch for filesystem builtins declared in the eval registry.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks::EvalDirectHook::call()`.
//!
//! Key details:
//! - This dispatcher keeps filesystem call lowering area-scoped while per-builtin
//!   metadata lives in individual declaration files.

use super::super::super::*;
use super::*;

/// Routes direct expression-level filesystem builtin calls through per-builtin leaf wrappers.
pub(in crate::interpreter) fn eval_builtin_filesystem_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "basename" => super::basename::eval_basename_declared_call(args, context, scope, values),
        "chdir" => super::chdir::eval_chdir_declared_call(args, context, scope, values),
        "chgrp" => super::chgrp::eval_chgrp_declared_call(args, context, scope, values),
        "chmod" => super::chmod::eval_chmod_declared_call(args, context, scope, values),
        "chown" => super::chown::eval_chown_declared_call(args, context, scope, values),
        "clearstatcache" => super::clearstatcache::eval_clearstatcache_declared_call(args, context, scope, values),
        "closedir" => super::closedir::eval_closedir_declared_call(args, context, scope, values),
        "copy" => super::copy::eval_copy_declared_call(args, context, scope, values),
        "dirname" => super::dirname::eval_dirname_declared_call(args, context, scope, values),
        "disk_free_space" => super::disk_free_space::eval_disk_free_space_declared_call(args, context, scope, values),
        "disk_total_space" => super::disk_total_space::eval_disk_total_space_declared_call(args, context, scope, values),
        "fclose" => super::fclose::eval_fclose_declared_call(args, context, scope, values),
        "fdatasync" => super::fdatasync::eval_fdatasync_declared_call(args, context, scope, values),
        "feof" => super::feof::eval_feof_declared_call(args, context, scope, values),
        "fflush" => super::fflush::eval_fflush_declared_call(args, context, scope, values),
        "fgetc" => super::fgetc::eval_fgetc_declared_call(args, context, scope, values),
        "fgetcsv" => super::fgetcsv::eval_fgetcsv_declared_call(args, context, scope, values),
        "fgets" => super::fgets::eval_fgets_declared_call(args, context, scope, values),
        "file" => super::file::eval_file_declared_call(args, context, scope, values),
        "file_exists" => super::file_exists::eval_file_exists_declared_call(args, context, scope, values),
        "file_get_contents" => super::file_get_contents::eval_file_get_contents_declared_call(args, context, scope, values),
        "file_put_contents" => super::file_put_contents::eval_file_put_contents_declared_call(args, context, scope, values),
        "fileatime" => super::fileatime::eval_fileatime_declared_call(args, context, scope, values),
        "filectime" => super::filectime::eval_filectime_declared_call(args, context, scope, values),
        "filegroup" => super::filegroup::eval_filegroup_declared_call(args, context, scope, values),
        "fileinode" => super::fileinode::eval_fileinode_declared_call(args, context, scope, values),
        "filemtime" => super::filemtime::eval_filemtime_declared_call(args, context, scope, values),
        "fileowner" => super::fileowner::eval_fileowner_declared_call(args, context, scope, values),
        "fileperms" => super::fileperms::eval_fileperms_declared_call(args, context, scope, values),
        "filesize" => super::filesize::eval_filesize_declared_call(args, context, scope, values),
        "filetype" => super::filetype::eval_filetype_declared_call(args, context, scope, values),
        "flock" => super::flock::eval_flock_declared_call(args, context, scope, values),
        "fnmatch" => super::fnmatch::eval_fnmatch_declared_call(args, context, scope, values),
        "fopen" => super::fopen::eval_fopen_declared_call(args, context, scope, values),
        "fpassthru" => super::fpassthru::eval_fpassthru_declared_call(args, context, scope, values),
        "fprintf" => super::fprintf::eval_fprintf_declared_call(args, context, scope, values),
        "fputcsv" => super::fputcsv::eval_fputcsv_declared_call(args, context, scope, values),
        "fread" => super::fread::eval_fread_declared_call(args, context, scope, values),
        "fscanf" => super::fscanf::eval_fscanf_declared_call(args, context, scope, values),
        "fseek" => super::fseek::eval_fseek_declared_call(args, context, scope, values),
        "fsockopen" => super::fsockopen::eval_fsockopen_declared_call(args, context, scope, values),
        "fstat" => super::fstat::eval_fstat_declared_call(args, context, scope, values),
        "fsync" => super::fsync::eval_fsync_declared_call(args, context, scope, values),
        "ftell" => super::ftell::eval_ftell_declared_call(args, context, scope, values),
        "ftruncate" => super::ftruncate::eval_ftruncate_declared_call(args, context, scope, values),
        "fwrite" => super::fwrite::eval_fwrite_declared_call(args, context, scope, values),
        "getcwd" => super::getcwd::eval_getcwd_declared_call(args, context, scope, values),
        "glob" => super::glob::eval_glob_declared_call(args, context, scope, values),
        "is_dir" => super::is_dir::eval_is_dir_declared_call(args, context, scope, values),
        "is_executable" => super::is_executable::eval_is_executable_declared_call(args, context, scope, values),
        "is_file" => super::is_file::eval_is_file_declared_call(args, context, scope, values),
        "is_link" => super::is_link::eval_is_link_declared_call(args, context, scope, values),
        "is_readable" => super::is_readable::eval_is_readable_declared_call(args, context, scope, values),
        "is_writable" => super::is_writable::eval_is_writable_declared_call(args, context, scope, values),
        "is_writeable" => super::is_writeable::eval_is_writeable_declared_call(args, context, scope, values),
        "lchgrp" => super::lchgrp::eval_lchgrp_declared_call(args, context, scope, values),
        "lchown" => super::lchown::eval_lchown_declared_call(args, context, scope, values),
        "link" => super::link::eval_link_declared_call(args, context, scope, values),
        "linkinfo" => super::linkinfo::eval_linkinfo_declared_call(args, context, scope, values),
        "lstat" => super::lstat::eval_lstat_declared_call(args, context, scope, values),
        "mkdir" => super::mkdir::eval_mkdir_declared_call(args, context, scope, values),
        "opendir" => super::opendir::eval_opendir_declared_call(args, context, scope, values),
        "pathinfo" => super::pathinfo::eval_pathinfo_declared_call(args, context, scope, values),
        "pclose" => super::pclose::eval_pclose_declared_call(args, context, scope, values),
        "pfsockopen" => super::pfsockopen::eval_pfsockopen_declared_call(args, context, scope, values),
        "popen" => super::popen::eval_popen_declared_call(args, context, scope, values),
        "readdir" => super::readdir::eval_readdir_declared_call(args, context, scope, values),
        "readfile" => super::readfile::eval_readfile_declared_call(args, context, scope, values),
        "readline" => super::readline::eval_readline_declared_call(args, context, scope, values),
        "readlink" => super::readlink::eval_readlink_declared_call(args, context, scope, values),
        "realpath" => super::realpath::eval_realpath_declared_call(args, context, scope, values),
        "realpath_cache_get" => super::realpath_cache_get::eval_realpath_cache_get_declared_call(args, context, scope, values),
        "realpath_cache_size" => super::realpath_cache_size::eval_realpath_cache_size_declared_call(args, context, scope, values),
        "rename" => super::rename::eval_rename_declared_call(args, context, scope, values),
        "rewind" => super::rewind::eval_rewind_declared_call(args, context, scope, values),
        "rewinddir" => super::rewinddir::eval_rewinddir_declared_call(args, context, scope, values),
        "rmdir" => super::rmdir::eval_rmdir_declared_call(args, context, scope, values),
        "scandir" => super::scandir::eval_scandir_declared_call(args, context, scope, values),
        "stat" => super::stat::eval_stat_declared_call(args, context, scope, values),
        "stream_bucket_append" => super::stream_bucket_append::eval_stream_bucket_append_declared_call(args, context, scope, values),
        "stream_bucket_make_writeable" => super::stream_bucket_make_writeable::eval_stream_bucket_make_writeable_declared_call(args, context, scope, values),
        "stream_bucket_new" => super::stream_bucket_new::eval_stream_bucket_new_declared_call(args, context, scope, values),
        "stream_bucket_prepend" => super::stream_bucket_prepend::eval_stream_bucket_prepend_declared_call(args, context, scope, values),
        "stream_context_create" => super::stream_context_create::eval_stream_context_create_declared_call(args, context, scope, values),
        "stream_context_get_default" => super::stream_context_get_default::eval_stream_context_get_default_declared_call(args, context, scope, values),
        "stream_context_get_options" => super::stream_context_get_options::eval_stream_context_get_options_declared_call(args, context, scope, values),
        "stream_context_get_params" => super::stream_context_get_params::eval_stream_context_get_params_declared_call(args, context, scope, values),
        "stream_context_set_default" => super::stream_context_set_default::eval_stream_context_set_default_declared_call(args, context, scope, values),
        "stream_context_set_option" => super::stream_context_set_option::eval_stream_context_set_option_declared_call(args, context, scope, values),
        "stream_context_set_params" => super::stream_context_set_params::eval_stream_context_set_params_declared_call(args, context, scope, values),
        "stream_copy_to_stream" => super::stream_copy_to_stream::eval_stream_copy_to_stream_declared_call(args, context, scope, values),
        "stream_filter_append" => super::stream_filter_append::eval_stream_filter_append_declared_call(args, context, scope, values),
        "stream_filter_prepend" => super::stream_filter_prepend::eval_stream_filter_prepend_declared_call(args, context, scope, values),
        "stream_filter_register" => super::stream_filter_register::eval_stream_filter_register_declared_call(args, context, scope, values),
        "stream_filter_remove" => super::stream_filter_remove::eval_stream_filter_remove_declared_call(args, context, scope, values),
        "stream_get_contents" => super::stream_get_contents::eval_stream_get_contents_declared_call(args, context, scope, values),
        "stream_get_line" => super::stream_get_line::eval_stream_get_line_declared_call(args, context, scope, values),
        "stream_get_meta_data" => super::stream_get_meta_data::eval_stream_get_meta_data_declared_call(args, context, scope, values),
        "stream_isatty" => super::stream_isatty::eval_stream_isatty_declared_call(args, context, scope, values),
        "stream_resolve_include_path" => super::stream_resolve_include_path::eval_stream_resolve_include_path_declared_call(args, context, scope, values),
        "stream_select" => super::stream_select::eval_stream_select_declared_call(args, context, scope, values),
        "stream_set_blocking" => super::stream_set_blocking::eval_stream_set_blocking_declared_call(args, context, scope, values),
        "stream_set_chunk_size" => super::stream_set_chunk_size::eval_stream_set_chunk_size_declared_call(args, context, scope, values),
        "stream_set_read_buffer" => super::stream_set_read_buffer::eval_stream_set_read_buffer_declared_call(args, context, scope, values),
        "stream_set_timeout" => super::stream_set_timeout::eval_stream_set_timeout_declared_call(args, context, scope, values),
        "stream_set_write_buffer" => super::stream_set_write_buffer::eval_stream_set_write_buffer_declared_call(args, context, scope, values),
        "stream_socket_accept" => super::stream_socket_accept::eval_stream_socket_accept_declared_call(args, context, scope, values),
        "stream_socket_client" => super::stream_socket_client::eval_stream_socket_client_declared_call(args, context, scope, values),
        "stream_socket_enable_crypto" => super::stream_socket_enable_crypto::eval_stream_socket_enable_crypto_declared_call(args, context, scope, values),
        "stream_socket_get_name" => super::stream_socket_get_name::eval_stream_socket_get_name_declared_call(args, context, scope, values),
        "stream_socket_pair" => super::stream_socket_pair::eval_stream_socket_pair_declared_call(args, context, scope, values),
        "stream_socket_recvfrom" => super::stream_socket_recvfrom::eval_stream_socket_recvfrom_declared_call(args, context, scope, values),
        "stream_socket_sendto" => super::stream_socket_sendto::eval_stream_socket_sendto_declared_call(args, context, scope, values),
        "stream_socket_server" => super::stream_socket_server::eval_stream_socket_server_declared_call(args, context, scope, values),
        "stream_socket_shutdown" => super::stream_socket_shutdown::eval_stream_socket_shutdown_declared_call(args, context, scope, values),
        "stream_wrapper_register" => super::stream_wrapper_register::eval_stream_wrapper_register_declared_call(args, context, scope, values),
        "stream_wrapper_restore" => super::stream_wrapper_restore::eval_stream_wrapper_restore_declared_call(args, context, scope, values),
        "stream_wrapper_unregister" => super::stream_wrapper_unregister::eval_stream_wrapper_unregister_declared_call(args, context, scope, values),
        "symlink" => super::symlink::eval_symlink_declared_call(args, context, scope, values),
        "sys_get_temp_dir" => super::sys_get_temp_dir::eval_sys_get_temp_dir_declared_call(args, context, scope, values),
        "tempnam" => super::tempnam::eval_tempnam_declared_call(args, context, scope, values),
        "tmpfile" => super::tmpfile::eval_tmpfile_declared_call(args, context, scope, values),
        "touch" => super::touch::eval_touch_declared_call(args, context, scope, values),
        "umask" => super::umask::eval_umask_declared_call(args, context, scope, values),
        "unlink" => super::unlink::eval_unlink_declared_call(args, context, scope, values),
        "vfprintf" => super::vfprintf::eval_vfprintf_declared_call(args, context, scope, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches direct expression-level calls for declaratively migrated filesystem builtins.
pub(in crate::interpreter) fn eval_builtin_filesystem_call_impl(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "basename" => super::basename::eval_basename_declared_call(args, context, scope, values),
        "fgetcsv" => eval_builtin_fgetcsv(args, context, scope, values),
        "fclose" | "fgetc" | "fgets" | "feof" | "fflush" | "fpassthru" | "fsync"
        | "fdatasync" | "ftell" | "rewind" | "fstat" | "stream_get_meta_data" => {
            eval_builtin_unary_stream(name, args, context, scope, values)
        }
        "fnmatch" => eval_builtin_fnmatch(args, context, scope, values),
        "fprintf" => eval_builtin_fprintf(args, context, scope, values),
        "fputcsv" => eval_builtin_fputcsv(args, context, scope, values),
        "fread" => eval_builtin_fread(args, context, scope, values),
        "fscanf" => eval_builtin_fscanf(args, context, scope, values),
        "fseek" => eval_builtin_fseek(args, context, scope, values),
        "ftruncate" => eval_builtin_ftruncate(args, context, scope, values),
        "fwrite" => eval_builtin_fwrite(args, context, scope, values),
        "readline" => eval_builtin_readline(args, context, scope, values),
        "realpath_cache_get" => eval_builtin_realpath_cache_get(args, values),
        "realpath_cache_size" => eval_builtin_realpath_cache_size(args, values),
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
        "vfprintf" => eval_builtin_vfprintf(args, context, scope, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
