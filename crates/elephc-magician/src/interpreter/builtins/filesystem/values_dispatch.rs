//! Purpose:
//! Routes evaluated-argument filesystem registry hooks to focused value dispatchers.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks::EvalValuesHook::call()`.
//!
//! Key details:
//! - Values hooks run after named/default argument binding has produced PHP
//!   parameter order.

use super::super::super::*;

use super::path_values_dispatch::eval_filesystem_path_values_result;
use super::stream_values_dispatch::eval_filesystem_stream_values_result;

/// Routes evaluated-argument filesystem builtin calls through per-builtin leaf wrappers.
pub(in crate::interpreter) fn eval_filesystem_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "basename" => super::basename::eval_basename_declared_values_result(evaluated_args, context, values),
        "chdir" => super::chdir::eval_chdir_declared_values_result(evaluated_args, context, values),
        "chgrp" => super::chgrp::eval_chgrp_declared_values_result(evaluated_args, context, values),
        "chmod" => super::chmod::eval_chmod_declared_values_result(evaluated_args, context, values),
        "chown" => super::chown::eval_chown_declared_values_result(evaluated_args, context, values),
        "clearstatcache" => super::clearstatcache::eval_clearstatcache_declared_values_result(evaluated_args, context, values),
        "closedir" => super::closedir::eval_closedir_declared_values_result(evaluated_args, context, values),
        "copy" => super::copy::eval_copy_declared_values_result(evaluated_args, context, values),
        "dirname" => super::dirname::eval_dirname_declared_values_result(evaluated_args, context, values),
        "disk_free_space" => super::disk_free_space::eval_disk_free_space_declared_values_result(evaluated_args, context, values),
        "disk_total_space" => super::disk_total_space::eval_disk_total_space_declared_values_result(evaluated_args, context, values),
        "fclose" => super::fclose::eval_fclose_declared_values_result(evaluated_args, context, values),
        "fdatasync" => super::fdatasync::eval_fdatasync_declared_values_result(evaluated_args, context, values),
        "feof" => super::feof::eval_feof_declared_values_result(evaluated_args, context, values),
        "fflush" => super::fflush::eval_fflush_declared_values_result(evaluated_args, context, values),
        "fgetc" => super::fgetc::eval_fgetc_declared_values_result(evaluated_args, context, values),
        "fgetcsv" => super::fgetcsv::eval_fgetcsv_declared_values_result(evaluated_args, context, values),
        "fgets" => super::fgets::eval_fgets_declared_values_result(evaluated_args, context, values),
        "file" => super::file::eval_file_declared_values_result(evaluated_args, context, values),
        "file_exists" => super::file_exists::eval_file_exists_declared_values_result(evaluated_args, context, values),
        "file_get_contents" => super::file_get_contents::eval_file_get_contents_declared_values_result(evaluated_args, context, values),
        "file_put_contents" => super::file_put_contents::eval_file_put_contents_declared_values_result(evaluated_args, context, values),
        "fileatime" => super::fileatime::eval_fileatime_declared_values_result(evaluated_args, context, values),
        "filectime" => super::filectime::eval_filectime_declared_values_result(evaluated_args, context, values),
        "filegroup" => super::filegroup::eval_filegroup_declared_values_result(evaluated_args, context, values),
        "fileinode" => super::fileinode::eval_fileinode_declared_values_result(evaluated_args, context, values),
        "filemtime" => super::filemtime::eval_filemtime_declared_values_result(evaluated_args, context, values),
        "fileowner" => super::fileowner::eval_fileowner_declared_values_result(evaluated_args, context, values),
        "fileperms" => super::fileperms::eval_fileperms_declared_values_result(evaluated_args, context, values),
        "filesize" => super::filesize::eval_filesize_declared_values_result(evaluated_args, context, values),
        "filetype" => super::filetype::eval_filetype_declared_values_result(evaluated_args, context, values),
        "flock" => super::flock::eval_flock_declared_values_result(evaluated_args, context, values),
        "fnmatch" => super::fnmatch::eval_fnmatch_declared_values_result(evaluated_args, context, values),
        "fopen" => super::fopen::eval_fopen_declared_values_result(evaluated_args, context, values),
        "fpassthru" => super::fpassthru::eval_fpassthru_declared_values_result(evaluated_args, context, values),
        "fprintf" => super::fprintf::eval_fprintf_declared_values_result(evaluated_args, context, values),
        "fputcsv" => super::fputcsv::eval_fputcsv_declared_values_result(evaluated_args, context, values),
        "fread" => super::fread::eval_fread_declared_values_result(evaluated_args, context, values),
        "fscanf" => super::fscanf::eval_fscanf_declared_values_result(evaluated_args, context, values),
        "fseek" => super::fseek::eval_fseek_declared_values_result(evaluated_args, context, values),
        "fsockopen" => super::fsockopen::eval_fsockopen_declared_values_result(evaluated_args, context, values),
        "fstat" => super::fstat::eval_fstat_declared_values_result(evaluated_args, context, values),
        "fsync" => super::fsync::eval_fsync_declared_values_result(evaluated_args, context, values),
        "ftell" => super::ftell::eval_ftell_declared_values_result(evaluated_args, context, values),
        "ftruncate" => super::ftruncate::eval_ftruncate_declared_values_result(evaluated_args, context, values),
        "fwrite" => super::fwrite::eval_fwrite_declared_values_result(evaluated_args, context, values),
        "getcwd" => super::getcwd::eval_getcwd_declared_values_result(evaluated_args, context, values),
        "glob" => super::glob::eval_glob_declared_values_result(evaluated_args, context, values),
        "is_dir" => super::is_dir::eval_is_dir_declared_values_result(evaluated_args, context, values),
        "is_executable" => super::is_executable::eval_is_executable_declared_values_result(evaluated_args, context, values),
        "is_file" => super::is_file::eval_is_file_declared_values_result(evaluated_args, context, values),
        "is_link" => super::is_link::eval_is_link_declared_values_result(evaluated_args, context, values),
        "is_readable" => super::is_readable::eval_is_readable_declared_values_result(evaluated_args, context, values),
        "is_writable" => super::is_writable::eval_is_writable_declared_values_result(evaluated_args, context, values),
        "is_writeable" => super::is_writeable::eval_is_writeable_declared_values_result(evaluated_args, context, values),
        "lchgrp" => super::lchgrp::eval_lchgrp_declared_values_result(evaluated_args, context, values),
        "lchown" => super::lchown::eval_lchown_declared_values_result(evaluated_args, context, values),
        "link" => super::link::eval_link_declared_values_result(evaluated_args, context, values),
        "linkinfo" => super::linkinfo::eval_linkinfo_declared_values_result(evaluated_args, context, values),
        "lstat" => super::lstat::eval_lstat_declared_values_result(evaluated_args, context, values),
        "mkdir" => super::mkdir::eval_mkdir_declared_values_result(evaluated_args, context, values),
        "opendir" => super::opendir::eval_opendir_declared_values_result(evaluated_args, context, values),
        "pathinfo" => super::pathinfo::eval_pathinfo_declared_values_result(evaluated_args, context, values),
        "pclose" => super::pclose::eval_pclose_declared_values_result(evaluated_args, context, values),
        "pfsockopen" => super::pfsockopen::eval_pfsockopen_declared_values_result(evaluated_args, context, values),
        "popen" => super::popen::eval_popen_declared_values_result(evaluated_args, context, values),
        "readdir" => super::readdir::eval_readdir_declared_values_result(evaluated_args, context, values),
        "readfile" => super::readfile::eval_readfile_declared_values_result(evaluated_args, context, values),
        "readline" => super::readline::eval_readline_declared_values_result(evaluated_args, context, values),
        "readlink" => super::readlink::eval_readlink_declared_values_result(evaluated_args, context, values),
        "realpath" => super::realpath::eval_realpath_declared_values_result(evaluated_args, context, values),
        "realpath_cache_get" => super::realpath_cache_get::eval_realpath_cache_get_declared_values_result(evaluated_args, context, values),
        "realpath_cache_size" => super::realpath_cache_size::eval_realpath_cache_size_declared_values_result(evaluated_args, context, values),
        "rename" => super::rename::eval_rename_declared_values_result(evaluated_args, context, values),
        "rewind" => super::rewind::eval_rewind_declared_values_result(evaluated_args, context, values),
        "rewinddir" => super::rewinddir::eval_rewinddir_declared_values_result(evaluated_args, context, values),
        "rmdir" => super::rmdir::eval_rmdir_declared_values_result(evaluated_args, context, values),
        "scandir" => super::scandir::eval_scandir_declared_values_result(evaluated_args, context, values),
        "stat" => super::stat::eval_stat_declared_values_result(evaluated_args, context, values),
        "stream_bucket_append" => super::stream_bucket_append::eval_stream_bucket_append_declared_values_result(evaluated_args, context, values),
        "stream_bucket_make_writeable" => super::stream_bucket_make_writeable::eval_stream_bucket_make_writeable_declared_values_result(evaluated_args, context, values),
        "stream_bucket_new" => super::stream_bucket_new::eval_stream_bucket_new_declared_values_result(evaluated_args, context, values),
        "stream_bucket_prepend" => super::stream_bucket_prepend::eval_stream_bucket_prepend_declared_values_result(evaluated_args, context, values),
        "stream_context_create" => super::stream_context_create::eval_stream_context_create_declared_values_result(evaluated_args, context, values),
        "stream_context_get_default" => super::stream_context_get_default::eval_stream_context_get_default_declared_values_result(evaluated_args, context, values),
        "stream_context_get_options" => super::stream_context_get_options::eval_stream_context_get_options_declared_values_result(evaluated_args, context, values),
        "stream_context_get_params" => super::stream_context_get_params::eval_stream_context_get_params_declared_values_result(evaluated_args, context, values),
        "stream_context_set_default" => super::stream_context_set_default::eval_stream_context_set_default_declared_values_result(evaluated_args, context, values),
        "stream_context_set_option" => super::stream_context_set_option::eval_stream_context_set_option_declared_values_result(evaluated_args, context, values),
        "stream_context_set_params" => super::stream_context_set_params::eval_stream_context_set_params_declared_values_result(evaluated_args, context, values),
        "stream_copy_to_stream" => super::stream_copy_to_stream::eval_stream_copy_to_stream_declared_values_result(evaluated_args, context, values),
        "stream_filter_append" => super::stream_filter_append::eval_stream_filter_append_declared_values_result(evaluated_args, context, values),
        "stream_filter_prepend" => super::stream_filter_prepend::eval_stream_filter_prepend_declared_values_result(evaluated_args, context, values),
        "stream_filter_register" => super::stream_filter_register::eval_stream_filter_register_declared_values_result(evaluated_args, context, values),
        "stream_filter_remove" => super::stream_filter_remove::eval_stream_filter_remove_declared_values_result(evaluated_args, context, values),
        "stream_get_contents" => super::stream_get_contents::eval_stream_get_contents_declared_values_result(evaluated_args, context, values),
        "stream_get_line" => super::stream_get_line::eval_stream_get_line_declared_values_result(evaluated_args, context, values),
        "stream_get_meta_data" => super::stream_get_meta_data::eval_stream_get_meta_data_declared_values_result(evaluated_args, context, values),
        "stream_isatty" => super::stream_isatty::eval_stream_isatty_declared_values_result(evaluated_args, context, values),
        "stream_resolve_include_path" => super::stream_resolve_include_path::eval_stream_resolve_include_path_declared_values_result(evaluated_args, context, values),
        "stream_select" => super::stream_select::eval_stream_select_declared_values_result(evaluated_args, context, values),
        "stream_set_blocking" => super::stream_set_blocking::eval_stream_set_blocking_declared_values_result(evaluated_args, context, values),
        "stream_set_chunk_size" => super::stream_set_chunk_size::eval_stream_set_chunk_size_declared_values_result(evaluated_args, context, values),
        "stream_set_read_buffer" => super::stream_set_read_buffer::eval_stream_set_read_buffer_declared_values_result(evaluated_args, context, values),
        "stream_set_timeout" => super::stream_set_timeout::eval_stream_set_timeout_declared_values_result(evaluated_args, context, values),
        "stream_set_write_buffer" => super::stream_set_write_buffer::eval_stream_set_write_buffer_declared_values_result(evaluated_args, context, values),
        "stream_socket_accept" => super::stream_socket_accept::eval_stream_socket_accept_declared_values_result(evaluated_args, context, values),
        "stream_socket_client" => super::stream_socket_client::eval_stream_socket_client_declared_values_result(evaluated_args, context, values),
        "stream_socket_enable_crypto" => super::stream_socket_enable_crypto::eval_stream_socket_enable_crypto_declared_values_result(evaluated_args, context, values),
        "stream_socket_get_name" => super::stream_socket_get_name::eval_stream_socket_get_name_declared_values_result(evaluated_args, context, values),
        "stream_socket_pair" => super::stream_socket_pair::eval_stream_socket_pair_declared_values_result(evaluated_args, context, values),
        "stream_socket_recvfrom" => super::stream_socket_recvfrom::eval_stream_socket_recvfrom_declared_values_result(evaluated_args, context, values),
        "stream_socket_sendto" => super::stream_socket_sendto::eval_stream_socket_sendto_declared_values_result(evaluated_args, context, values),
        "stream_socket_server" => super::stream_socket_server::eval_stream_socket_server_declared_values_result(evaluated_args, context, values),
        "stream_socket_shutdown" => super::stream_socket_shutdown::eval_stream_socket_shutdown_declared_values_result(evaluated_args, context, values),
        "stream_wrapper_register" => super::stream_wrapper_register::eval_stream_wrapper_register_declared_values_result(evaluated_args, context, values),
        "stream_wrapper_restore" => super::stream_wrapper_restore::eval_stream_wrapper_restore_declared_values_result(evaluated_args, context, values),
        "stream_wrapper_unregister" => super::stream_wrapper_unregister::eval_stream_wrapper_unregister_declared_values_result(evaluated_args, context, values),
        "symlink" => super::symlink::eval_symlink_declared_values_result(evaluated_args, context, values),
        "sys_get_temp_dir" => super::sys_get_temp_dir::eval_sys_get_temp_dir_declared_values_result(evaluated_args, context, values),
        "tempnam" => super::tempnam::eval_tempnam_declared_values_result(evaluated_args, context, values),
        "tmpfile" => super::tmpfile::eval_tmpfile_declared_values_result(evaluated_args, context, values),
        "touch" => super::touch::eval_touch_declared_values_result(evaluated_args, context, values),
        "umask" => super::umask::eval_umask_declared_values_result(evaluated_args, context, values),
        "unlink" => super::unlink::eval_unlink_declared_values_result(evaluated_args, context, values),
        "vfprintf" => super::vfprintf::eval_vfprintf_declared_values_result(evaluated_args, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches evaluated-argument calls for declaratively migrated filesystem builtins.
pub(in crate::interpreter) fn eval_filesystem_values_result_impl(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) =
        eval_filesystem_path_values_result(name, evaluated_args, context, values)?
    {
        return Ok(result);
    }
    if let Some(result) =
        eval_filesystem_stream_values_result(name, evaluated_args, context, values)?
    {
        return Ok(result);
    }
    Err(EvalStatus::RuntimeFatal)
}
