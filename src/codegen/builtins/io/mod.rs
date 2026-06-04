//! Purpose:
//! Dispatches filesystem, path, stream, and diagnostic PHP builtins to their focused codegen emitters.
//! Keeps the public builtin category surface small while leaf files own lowering details.
//!
//! Called from:
//! - `crate::codegen::builtins::emit_builtin_call()`.
//!
//! Key details:
//! - Dispatcher names must stay aligned with the builtin catalog and signature normalization layer.

mod basename;
mod chdir;
mod chgrp;
mod chmod;
mod chown;
mod clearstatcache;
mod copy;
mod disk_space;
mod dirname;
mod fclose;
mod fdatasync;
mod feof;
mod fflush;
mod fgetc;
mod fgetcsv;
mod fgets;
mod flock;
mod fnmatch;
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
mod data_stream;
mod compress_bzip2_stream;
mod compress_zlib_stream;
mod ftp_stream;
mod ftps_stream;
mod http_stream;
mod https_stream;
mod phar_stream;
mod php_filter_stream;
mod filesize;
mod filetype;
mod fopen;
mod fsockopen;
mod gethostname;
mod gethostbyname;
mod gethostbyaddr;
mod getprotobyname;
mod getprotobynumber;
mod getservbyname;
mod getservbyport;
mod fpassthru;
mod fprintf;
mod vfprintf;
mod fputcsv;
mod fread;
mod fscanf;
mod readfile;
mod readlink;
mod fseek;
mod fsync;
mod ftell;
mod ftruncate;
mod fwrite;
mod getcwd;
mod glob_fn;
mod is_dir;
mod is_executable;
mod is_file;
mod is_link;
mod is_readable;
mod is_writable;
mod link;
mod linkinfo;
mod mkdir;
mod pathinfo;
mod path_op_wrapper;
mod pclose;
mod popen;
mod opendir;
mod readdir;
mod closedir;
mod rewinddir;
mod print_r;
mod readline;
mod realpath;
mod rename;
mod rewind;
mod rmdir;
mod fstat;
mod lstat;
mod scandir;
mod stat;
mod stat_result;
pub(crate) mod stream_arg;
mod stream_copy_to_stream;
mod stream_get_line;
mod stream_socket_accept;
mod stream_socket_client;
mod stream_socket_get_name;
mod stream_socket_pair;
mod stream_socket_recvfrom;
mod stream_socket_sendto;
mod stream_socket_server;
mod stream_socket_shutdown;
mod stream_context_create;
mod stream_context_get_default;
mod stream_context_get_options;
mod stream_context_get_params;
mod stream_context_set_default;
mod stream_context_set_option;
mod stream_context_set_params;
mod stream_notification;
mod stream_resolve_include_path;
mod stream_bucket;
mod stream_filter_register;
mod stream_set_buffer;
mod stream_socket_enable_crypto;
mod stream_wrapper_register;
mod stream_wrapper_restore;
mod stream_wrapper_unregister;
mod stream_get_contents;
mod stream_get_meta_data;
mod stream_introspection;
mod stream_filter;
pub(crate) mod stream_filter_bzip2;
pub(crate) mod stream_filter_iconv;
pub(crate) mod stream_filter_iconv_write;
pub(crate) mod stream_filter_inflate;
pub(crate) mod stream_filter_zlib;
mod stream_isatty;
mod stream_select;
mod stream_set_blocking;
mod stream_set_timeout;
mod symlink;
mod sys_get_temp_dir;
mod tempnam;
mod tmpfile;
mod touch;
mod umask;
mod unlink;
mod var_dump;

pub(crate) use https_stream::publish_tls_function_pointers;

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Dispatches a PHP builtin call by name to its focused codegen emitter.
///
/// `name` must match a catalogued PHP builtin in the `io` category (e.g., `fopen`,
/// `file_get_contents`, `copy`). The matching emitter receives the raw argument
/// expressions and emits target-specific assembly for the call.
///
/// Returns `Some(PhpType)` with the return type on successful dispatch, or `None`
/// if `name` is not a recognised io builtin.
///
/// # Arguments
/// - `name`    — lowercase ASCII builtin name (case-insensitive per PHP semantics)
/// - `args`    — parsed argument expressions from the call site
/// - `emitter` — target-aware assembly emitter (controls instruction emission)
/// - `ctx`     — shared codegen context (frame layout, locals, ownership)
/// - `data`    — writable data section for relocations, string tables, and metadata
pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    match name {
        "var_dump" => var_dump::emit(name, args, emitter, ctx, data),
        "print_r" => print_r::emit(name, args, emitter, ctx, data),
        "fopen" => fopen::emit(name, args, emitter, ctx, data),
        "fclose" => fclose::emit(name, args, emitter, ctx, data),
        "fread" => fread::emit(name, args, emitter, ctx, data),
        "fwrite" => fwrite::emit(name, args, emitter, ctx, data),
        "fgets" => fgets::emit(name, args, emitter, ctx, data),
        "fgetc" => fgetc::emit(name, args, emitter, ctx, data),
        "fprintf" => fprintf::emit(name, args, emitter, ctx, data),
        "vfprintf" => vfprintf::emit(name, args, emitter, ctx, data),
        "fscanf" => fscanf::emit(name, args, emitter, ctx, data),
        "fpassthru" => fpassthru::emit(name, args, emitter, ctx, data),
        "flock" => flock::emit(name, args, emitter, ctx, data),
        "tmpfile" => tmpfile::emit(name, args, emitter, ctx, data),
        "readfile" => readfile::emit(name, args, emitter, ctx, data),
        "symlink" => symlink::emit(name, args, emitter, ctx, data),
        "link" => link::emit(name, args, emitter, ctx, data),
        "readlink" => readlink::emit(name, args, emitter, ctx, data),
        "linkinfo" => linkinfo::emit(name, args, emitter, ctx, data),
        "feof" => feof::emit(name, args, emitter, ctx, data),
        "readline" => readline::emit(name, args, emitter, ctx, data),
        "fseek" => fseek::emit(name, args, emitter, ctx, data),
        "ftell" => ftell::emit(name, args, emitter, ctx, data),
        "rewind" => rewind::emit(name, args, emitter, ctx, data),
        "file_get_contents" => file_get_contents::emit(name, args, emitter, ctx, data),
        "file_put_contents" => file_put_contents::emit(name, args, emitter, ctx, data),
        "file" => file::emit(name, args, emitter, ctx, data),
        "file_exists" => file_exists::emit(name, args, emitter, ctx, data),
        "is_file" => is_file::emit(name, args, emitter, ctx, data),
        "is_dir" => is_dir::emit(name, args, emitter, ctx, data),
        "is_readable" => is_readable::emit(name, args, emitter, ctx, data),
        "is_writable" => is_writable::emit(name, args, emitter, ctx, data),
        "filesize" => filesize::emit(name, args, emitter, ctx, data),
        "filemtime" => filemtime::emit(name, args, emitter, ctx, data),
        "copy" => copy::emit(name, args, emitter, ctx, data),
        "disk_free_space" | "disk_total_space" => {
            disk_space::emit(name, args, emitter, ctx, data)
        }
        "rename" => rename::emit(name, args, emitter, ctx, data),
        "unlink" => unlink::emit(name, args, emitter, ctx, data),
        "mkdir" => mkdir::emit(name, args, emitter, ctx, data),
        "rmdir" => rmdir::emit(name, args, emitter, ctx, data),
        "scandir" => scandir::emit(name, args, emitter, ctx, data),
        "glob" => glob_fn::emit(name, args, emitter, ctx, data),
        "getcwd" => getcwd::emit(name, args, emitter, ctx, data),
        "chdir" => chdir::emit(name, args, emitter, ctx, data),
        "tempnam" => tempnam::emit(name, args, emitter, ctx, data),
        "sys_get_temp_dir" => sys_get_temp_dir::emit(name, args, emitter, ctx, data),
        "fgetcsv" => fgetcsv::emit(name, args, emitter, ctx, data),
        "fputcsv" => fputcsv::emit(name, args, emitter, ctx, data),
        "fileatime" => fileatime::emit(name, args, emitter, ctx, data),
        "filectime" => filectime::emit(name, args, emitter, ctx, data),
        "fileperms" => fileperms::emit(name, args, emitter, ctx, data),
        "fileowner" => fileowner::emit(name, args, emitter, ctx, data),
        "filegroup" => filegroup::emit(name, args, emitter, ctx, data),
        "fileinode" => fileinode::emit(name, args, emitter, ctx, data),
        "filetype" => filetype::emit(name, args, emitter, ctx, data),
        "is_executable" => is_executable::emit(name, args, emitter, ctx, data),
        "is_link" => is_link::emit(name, args, emitter, ctx, data),
        // is_writeable is a documented PHP alias of is_writable.
        "is_writeable" => is_writable::emit(name, args, emitter, ctx, data),
        "clearstatcache" => clearstatcache::emit(name, args, emitter, ctx, data),
        "stat" => stat::emit(name, args, emitter, ctx, data),
        "lstat" => lstat::emit(name, args, emitter, ctx, data),
        "fstat" => fstat::emit(name, args, emitter, ctx, data),
        "basename" => basename::emit(name, args, emitter, ctx, data),
        "dirname" => dirname::emit(name, args, emitter, ctx, data),
        "fnmatch" => fnmatch::emit(name, args, emitter, ctx, data),
        "realpath" => realpath::emit(name, args, emitter, ctx, data),
        "pathinfo" => pathinfo::emit(name, args, emitter, ctx, data),
        "chmod" => chmod::emit(name, args, emitter, ctx, data),
        "chown" => chown::emit(name, args, emitter, ctx, data),
        "chgrp" => chgrp::emit(name, args, emitter, ctx, data),
        "umask" => umask::emit(name, args, emitter, ctx, data),
        "ftruncate" => ftruncate::emit(name, args, emitter, ctx, data),
        "fsync" => fsync::emit(name, args, emitter, ctx, data),
        "fflush" => fflush::emit(name, args, emitter, ctx, data),
        "fdatasync" => fdatasync::emit(name, args, emitter, ctx, data),
        "touch" => touch::emit(name, args, emitter, ctx, data),
        "gethostname" => gethostname::emit(name, args, emitter, ctx, data),
        "gethostbyname" => gethostbyname::emit(name, args, emitter, ctx, data),
        "gethostbyaddr" => gethostbyaddr::emit(name, args, emitter, ctx, data),
        "getprotobyname" => getprotobyname::emit(name, args, emitter, ctx, data),
        "getprotobynumber" => getprotobynumber::emit(name, args, emitter, ctx, data),
        "getservbyname" => getservbyname::emit(name, args, emitter, ctx, data),
        "getservbyport" => getservbyport::emit(name, args, emitter, ctx, data),
        "stream_copy_to_stream" => {
            stream_copy_to_stream::emit(name, args, emitter, ctx, data)
        }
        "stream_get_contents" => stream_get_contents::emit(name, args, emitter, ctx, data),
        "stream_get_meta_data" => {
            stream_get_meta_data::emit(name, args, emitter, ctx, data)
        }
        "stream_get_line" => stream_get_line::emit(name, args, emitter, ctx, data),
        "stream_isatty" => stream_isatty::emit(name, args, emitter, ctx, data),
        "stream_select" => stream_select::emit(name, args, emitter, ctx, data),
        "stream_set_blocking" => stream_set_blocking::emit(name, args, emitter, ctx, data),
        "stream_set_timeout" => stream_set_timeout::emit(name, args, emitter, ctx, data),
        "stream_socket_server" => stream_socket_server::emit(name, args, emitter, ctx, data),
        "stream_socket_client" => stream_socket_client::emit(name, args, emitter, ctx, data),
        "fsockopen" | "pfsockopen" => fsockopen::emit(name, args, emitter, ctx, data),
        "stream_wrapper_register" => {
            stream_wrapper_register::emit(name, args, emitter, ctx, data)
        }
        "stream_wrapper_unregister" => {
            stream_wrapper_unregister::emit(name, args, emitter, ctx, data)
        }
        "stream_wrapper_restore" => {
            stream_wrapper_restore::emit(name, args, emitter, ctx, data)
        }
        "stream_socket_enable_crypto" => {
            stream_socket_enable_crypto::emit(name, args, emitter, ctx, data)
        }
        "stream_context_create" => {
            stream_context_create::emit(name, args, emitter, ctx, data)
        }
        "stream_context_get_default" => {
            stream_context_get_default::emit(name, args, emitter, ctx, data)
        }
        "stream_context_set_default" => {
            stream_context_set_default::emit(name, args, emitter, ctx, data)
        }
        "stream_context_set_option" => {
            stream_context_set_option::emit(name, args, emitter, ctx, data)
        }
        "stream_context_set_params" => {
            stream_context_set_params::emit(name, args, emitter, ctx, data)
        }
        "stream_context_get_options" => {
            stream_context_get_options::emit(name, args, emitter, ctx, data)
        }
        "stream_context_get_params" => {
            stream_context_get_params::emit(name, args, emitter, ctx, data)
        }
        "stream_resolve_include_path" => {
            stream_resolve_include_path::emit(name, args, emitter, ctx, data)
        }
        "stream_filter_register" => {
            stream_filter_register::emit(name, args, emitter, ctx, data)
        }
        "stream_bucket_new" => stream_bucket::emit_new(name, args, emitter, ctx, data),
        "stream_bucket_make_writeable" => {
            stream_bucket::emit_make_writeable(name, args, emitter, ctx, data)
        }
        "stream_bucket_append" | "stream_bucket_prepend" => {
            stream_bucket::emit_append_or_prepend(name, args, emitter, ctx, data)
        }
        "stream_set_chunk_size"
        | "stream_set_read_buffer"
        | "stream_set_write_buffer" => {
            stream_set_buffer::emit(name, args, emitter, ctx, data)
        }
        "stream_socket_accept" => stream_socket_accept::emit(name, args, emitter, ctx, data),
        "stream_socket_shutdown" => {
            stream_socket_shutdown::emit(name, args, emitter, ctx, data)
        }
        "stream_socket_sendto" => {
            stream_socket_sendto::emit(name, args, emitter, ctx, data)
        }
        "stream_socket_recvfrom" => {
            stream_socket_recvfrom::emit(name, args, emitter, ctx, data)
        }
        "stream_socket_get_name" => {
            stream_socket_get_name::emit(name, args, emitter, ctx, data)
        }
        "stream_socket_pair" => {
            stream_socket_pair::emit(name, args, emitter, ctx, data)
        }
        "popen" => popen::emit(name, args, emitter, ctx, data),
        "pclose" => pclose::emit(name, args, emitter, ctx, data),
        "opendir" => opendir::emit(name, args, emitter, ctx, data),
        "readdir" => readdir::emit(name, args, emitter, ctx, data),
        "closedir" => closedir::emit(name, args, emitter, ctx, data),
        "rewinddir" => rewinddir::emit(name, args, emitter, ctx, data),
        "stream_is_local" | "stream_supports_lock" | "stream_get_wrappers"
        | "stream_get_transports" | "stream_get_filters" => {
            stream_introspection::emit(name, args, emitter, ctx, data)
        }
        "stream_filter_append" | "stream_filter_prepend" => {
            stream_filter::emit_attach(name, args, emitter, ctx, data)
        }
        "stream_filter_remove" => stream_filter::emit_remove(name, args, emitter, ctx, data),
        _ => None,
    }
}
