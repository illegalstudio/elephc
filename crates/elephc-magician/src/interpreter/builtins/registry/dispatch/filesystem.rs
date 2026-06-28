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
    context: &mut ElephcEvalContext,
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
            eval_chmod_result(*filename, *permissions, context, values)?
        }
        "chown" | "chgrp" | "lchown" | "lchgrp" => {
            let [filename, principal] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_chown_like_result(name, *filename, *principal, context, values)?
        }
        "closedir" | "readdir" | "rewinddir" => {
            let [dir_handle] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_unary_directory_result(name, *dir_handle, context, values)?
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
            eval_file_probe_result(name, *filename, context, values)?
        }
        "fileatime" | "filectime" | "filegroup" | "fileinode" | "filemtime" | "fileowner"
        | "fileperms" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_stat_scalar_result(name, *filename, context, values)?
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
        "fclose"
        | "fgetc"
        | "fgets"
        | "feof"
        | "fflush"
        | "fpassthru"
        | "fsync"
        | "fdatasync"
        | "ftell"
        | "rewind"
        | "fstat"
        | "stream_get_meta_data" => {
            let [stream] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_unary_stream_result(name, *stream, context, values)?
        }
        "filesize" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_filesize_result(*filename, context, values)?
        }
        "filetype" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_filetype_result(*filename, context, values)?
        }
        "fnmatch" => match evaluated_args {
            [pattern, filename] => eval_fnmatch_result(*pattern, *filename, None, values)?,
            [pattern, filename, flags] => {
                eval_fnmatch_result(*pattern, *filename, Some(*flags), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "fgetcsv" => match evaluated_args {
            [stream] => eval_fgetcsv_result(*stream, None, None, context, values)?,
            [stream, length] => {
                eval_fgetcsv_result(*stream, Some(*length), None, context, values)?
            }
            [stream, length, separator] => {
                eval_fgetcsv_result(*stream, Some(*length), Some(*separator), context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "fopen" => {
            if !(2..=4).contains(&evaluated_args.len()) {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_fopen_result(evaluated_args[0], evaluated_args[1], context, values)?
        }
        "fputcsv" => match evaluated_args {
            [stream, fields] => eval_fputcsv_result(*stream, *fields, None, None, context, values)?,
            [stream, fields, separator] => {
                eval_fputcsv_result(*stream, *fields, Some(*separator), None, context, values)?
            }
            [stream, fields, separator, enclosure] => eval_fputcsv_result(
                *stream,
                *fields,
                Some(*separator),
                Some(*enclosure),
                context,
                values,
            )?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "fprintf" => {
            let Some((stream, rest)) = evaluated_args.split_first() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let Some((format, format_args)) = rest.split_first() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_fprintf_result(*stream, *format, format_args, context, values)?
        }
        "flock" => {
            if !(2..=3).contains(&evaluated_args.len()) {
                return Err(EvalStatus::RuntimeFatal);
            }
            let (success, _) = eval_flock_result(
                evaluated_args[0],
                evaluated_args[1],
                context,
                values,
            )?;
            values.bool_value(success)?
        }
        "fread" => {
            let [stream, length] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_fread_result(*stream, *length, context, values)?
        }
        "fsockopen" | "pfsockopen" => {
            if !(2..=5).contains(&evaluated_args.len()) {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_fsockopen_result(evaluated_args[0], evaluated_args[1], context, values)?
        }
        "fscanf" => {
            if evaluated_args.len() < 2 {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_fscanf_result(evaluated_args[0], evaluated_args[1], context, values)?
        }
        "fseek" => match evaluated_args {
            [stream, offset] => eval_fseek_result(*stream, *offset, None, context, values)?,
            [stream, offset, whence] => {
                eval_fseek_result(*stream, *offset, Some(*whence), context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "ftruncate" => {
            let [stream, size] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_ftruncate_result(*stream, *size, context, values)?
        }
        "fwrite" => {
            let [stream, data] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_fwrite_result(*stream, *data, context, values)?
        }
        "stat" | "lstat" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stat_array_result(name, *filename, context, values)?
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
        "readline" => {
            if evaluated_args.len() > 1 {
                return Err(EvalStatus::RuntimeFatal);
            }
            let prompt = evaluated_args.first().copied();
            eval_readline_result(prompt, values)?
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
        "opendir" => {
            let [directory] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_opendir_result(*directory, context, values)?
        }
        "pathinfo" => match evaluated_args {
            [path] => eval_pathinfo_result(*path, None, values)?,
            [path, flags] => eval_pathinfo_result(*path, Some(*flags), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "pclose" => {
            let [handle] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_pclose_result(*handle, context, values)?
        }
        "popen" => {
            let [command, mode] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_popen_result(*command, *mode, context, values)?
        }
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
        "stream_copy_to_stream" => match evaluated_args {
            [from, to] => {
                eval_stream_copy_to_stream_result(*from, *to, None, None, context, values)?
            }
            [from, to, length] => {
                eval_stream_copy_to_stream_result(*from, *to, Some(*length), None, context, values)?
            }
            [from, to, length, offset] => eval_stream_copy_to_stream_result(
                *from,
                *to,
                Some(*length),
                Some(*offset),
                context,
                values,
            )?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "stream_context_create" => match evaluated_args {
            [] => eval_stream_context_create_result(None, context, values)?,
            [options] => eval_stream_context_create_result(Some(*options), context, values)?,
            [options, _params] => {
                eval_stream_context_create_result(Some(*options), context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "stream_context_get_default" => {
            if evaluated_args.len() > 1 {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_stream_context_get_default_result(context, values)?
        }
        "stream_context_get_options" => {
            let [stream_context] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_context_get_options_result(*stream_context, context, values)?
        }
        "stream_context_get_params" => {
            let [stream_context] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            if values.type_tag(*stream_context)? != EVAL_TAG_RESOURCE {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.assoc_new(0)?
        }
        "stream_context_set_default" => {
            let [_options] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_context_get_default_result(context, values)?
        }
        "stream_context_set_option" => match evaluated_args {
            [stream_context, options] => {
                eval_stream_context_set_options_result(*stream_context, *options, context, values)?
            }
            [stream_context, wrapper, option, value] => eval_stream_context_set_option_result(
                *stream_context,
                *wrapper,
                *option,
                *value,
                context,
                values,
            )?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "stream_context_set_params" => {
            let [stream_context, _params] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            if values.type_tag(*stream_context)? != EVAL_TAG_RESOURCE {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.bool_value(true)?
        }
        "stream_wrapper_register" | "stream_wrapper_unregister" | "stream_wrapper_restore" => {
            eval_stream_wrapper_registry_result(name, evaluated_args, context, values)?
        }
        "stream_filter_register" => {
            let [filter_name, class] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_filter_register_result(*filter_name, *class, values)?
        }
        "stream_filter_append" | "stream_filter_prepend" => {
            if !(2..=4).contains(&evaluated_args.len()) {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_stream_filter_attach_result(
                name,
                evaluated_args[0],
                evaluated_args[1],
                context,
                values,
            )?
        }
        "stream_filter_remove" => {
            let [stream_filter] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_filter_remove_result(*stream_filter, context, values)?
        }
        "stream_bucket_new" => {
            let [stream, buffer] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_bucket_new_result(*stream, *buffer, context, values)?
        }
        "stream_bucket_make_writeable" => {
            let [brigade] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_bucket_make_writeable_result(*brigade, values)?
        }
        "stream_bucket_append" | "stream_bucket_prepend" => {
            let [brigade, bucket] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_bucket_push_result(name, *brigade, *bucket, values)?
        }
        "stream_select" => eval_stream_select_result(evaluated_args, values)?,
        "stream_socket_server" => {
            let [address] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_socket_server_result(*address, context, values)?
        }
        "stream_socket_client" => {
            let [address] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_socket_client_result(*address, context, values)?
        }
        "stream_socket_accept" => {
            if !(1..=3).contains(&evaluated_args.len()) {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_stream_socket_accept_result(evaluated_args[0], context, values)?
        }
        "stream_socket_get_name" => {
            let [socket, remote] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_socket_get_name_result(*socket, *remote, context, values)?
        }
        "stream_socket_shutdown" => {
            let [stream, mode] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_socket_shutdown_result(*stream, *mode, context, values)?
        }
        "stream_socket_enable_crypto" => {
            if !(2..=4).contains(&evaluated_args.len()) {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_stream_socket_enable_crypto_result(
                evaluated_args[0],
                evaluated_args[1],
                context,
                values,
            )?
        }
        "stream_socket_sendto" => {
            if !(2..=4).contains(&evaluated_args.len()) {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_stream_socket_sendto_result(evaluated_args[0], evaluated_args[1], context, values)?
        }
        "stream_socket_recvfrom" => {
            if !(2..=4).contains(&evaluated_args.len()) {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_stream_socket_recvfrom_result(
                evaluated_args[0],
                evaluated_args[1],
                context,
                values,
            )?
        }
        "stream_socket_pair" => {
            let [_domain, _socket_type, _protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_socket_pair_result(context, values)?
        }
        "stream_get_contents" => match evaluated_args {
            [stream] => eval_stream_get_contents_result(*stream, None, None, context, values)?,
            [stream, length] => {
                eval_stream_get_contents_result(*stream, Some(*length), None, context, values)?
            }
            [stream, length, offset] => eval_stream_get_contents_result(
                *stream,
                Some(*length),
                Some(*offset),
                context,
                values,
            )?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "stream_get_line" => match evaluated_args {
            [stream, length] => {
                eval_stream_get_line_result(*stream, *length, None, context, values)?
            }
            [stream, length, ending] => {
                eval_stream_get_line_result(*stream, *length, Some(*ending), context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "stream_isatty" => {
            let [stream] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_isatty_result(*stream, context, values)?
        }
        "stream_set_blocking" => {
            let [stream, enable] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_set_blocking_result(*stream, *enable, context, values)?
        }
        "stream_set_chunk_size" | "stream_set_read_buffer" | "stream_set_write_buffer" => {
            let [stream, size] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_set_buffer_like_result(name, *stream, *size, context, values)?
        }
        "stream_set_timeout" => match evaluated_args {
            [stream, seconds] => {
                eval_stream_set_timeout_result(*stream, *seconds, None, context, values)?
            }
            [stream, seconds, microseconds] => eval_stream_set_timeout_result(
                *stream,
                *seconds,
                Some(*microseconds),
                context,
                values,
            )?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
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
        "tmpfile" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_tmpfile_result(context, values)?
        }
        "vfprintf" => {
            let [stream, format, array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_vfprintf_result(*stream, *format, *array, context, values)?
        }
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
