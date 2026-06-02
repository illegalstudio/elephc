//! Purpose:
//! Collects file, directory, path, stat, CSV, glob, and descriptor runtime emitters.
//! The module owns re-export wiring for helpers that adapt PHP I/O builtins to libc and runtime arrays.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` during the I/O runtime section.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

mod basename;
mod cstr;
mod disk_space;
mod dirname;
mod dirname_levels;
mod feof;
mod fgetcsv;
mod fgets;
mod file;
mod file_get_contents;
mod file_put_contents;
mod fd_write;
mod fnmatch;
mod fopen;
mod fputcsv;
mod fread;
mod fwrite;
mod phar_read;
mod phar_write;
mod fs;
mod getcwd;
mod gethostname;
mod gethostbyname;
mod gethostbyaddr;
mod getprotobyname;
mod getprotobynumber;
mod getservbyname;
mod getservbyport;
mod glob;
mod protoent;
mod servent;
mod modify;
mod modify_x86_64;
mod pathinfo_array;
mod pathinfo_str;
mod realpath;
mod scandir;
mod stat;
mod stat_array;
mod stat_ext;
mod socket_addr;
mod resolve_host;
mod resolve_host_v6;
mod inet6_pton;
mod stream_socket_client_v6;
mod stream_socket_server_v6;
mod build_sockaddr_in6;
mod opendir_glob;
mod socket_scheme;
mod format_sockaddr;
mod data_stream;
mod stream_filter;
mod fsockopen;
mod ftp;
mod http;
mod https;
mod notification;
mod stream_wrapper_register;
mod stream_wrapper_unregister;
mod http_build_request;
mod stream_context_get_ssl_peer_name;
mod apply_socket_opts;
mod stream_context_get_int_option;
mod stream_context_get_string_option;
mod stream_context_set_option_4;
mod stream_copy_to_stream;
mod stream_get_contents;
mod stream_get_line;
mod stream_get_meta_data;
mod stream_socket_accept;
mod stream_socket_client;
mod pclose;
mod popen;
mod opendir;
mod readdir;
mod closedir;
mod rewinddir;
mod stream_socket_get_name;
mod stream_socket_pair;
mod stream_socket_recvfrom;
mod stream_socket_sendto;
mod socket_backlog;
mod stream_socket_server;
mod stream_socket_shutdown;
mod unix_socket_client;
mod unix_socket_server;
mod stream_isatty;
mod stream_select;
mod stream_set_blocking;
mod stream_set_timeout;
mod streams_ext;
mod symlink;
mod tempnam;
mod path_is_wrapper;
mod readfile_wrapper;
mod user_filter;
mod user_filter_brigade;
mod stash_connect_host;
mod touch_meta_array;
mod user_wrapper;
mod user_wrapper_cast;
mod user_wrapper_dir;
mod user_wrapper_path_op;
mod user_wrapper_set_option;
mod user_wrapper_url_stat;
mod var_dump_walk;

pub(crate) use basename::emit_basename;
pub(crate) use cstr::emit_cstr;
pub(crate) use disk_space::emit_disk_space;
pub(crate) use dirname::emit_dirname;
pub(crate) use dirname_levels::emit_dirname_levels;
pub(crate) use feof::emit_feof;
pub(crate) use fgetcsv::emit_fgetcsv;
pub(crate) use fgets::emit_fgets;
pub(crate) use file::emit_file;
pub(crate) use file_get_contents::emit_file_get_contents;
pub(crate) use fd_write::emit_fd_write;
pub(crate) use file_put_contents::emit_file_put_contents;
pub(crate) use fnmatch::emit_fnmatch;
pub(crate) use fopen::emit_fopen;
pub(crate) use fputcsv::emit_fputcsv;
pub(crate) use fread::emit_fread;
pub(crate) use fwrite::emit_fwrite;
pub(crate) use phar_read::emit_phar_read;
pub(crate) use phar_write::emit_phar_write;
pub(crate) use fs::emit_fs;
pub(crate) use getcwd::emit_getcwd;
pub(crate) use gethostname::emit_gethostname;
pub(crate) use gethostbyname::emit_gethostbyname;
pub(crate) use gethostbyaddr::emit_gethostbyaddr;
pub(crate) use getprotobyname::emit_getprotobyname;
pub(crate) use getprotobynumber::emit_getprotobynumber;
pub(crate) use getservbyname::emit_getservbyname;
pub(crate) use getservbyport::emit_getservbyport;
pub(crate) use glob::emit_glob;
pub(crate) use protoent::emit_protoent_load;
pub(crate) use servent::emit_servent_load;
pub(crate) use modify::emit_modify;
pub(crate) use pathinfo_array::emit_pathinfo_array;
pub(crate) use pathinfo_str::emit_pathinfo_str;
pub(crate) use realpath::emit_realpath;
pub(crate) use scandir::emit_scandir;
pub(crate) use stat::emit_stat;
pub(crate) use stat_array::emit_stat_array;
pub(crate) use stat_ext::emit_stat_ext;
pub(crate) use socket_addr::emit_inet_addr_parse;
pub(crate) use resolve_host::emit_resolve_host;
pub(crate) use resolve_host_v6::emit_resolve_host_v6;
pub(crate) use inet6_pton::emit_inet6_pton;
pub(crate) use stream_socket_client_v6::emit_stream_socket_client_v6;
pub(crate) use stream_socket_server_v6::emit_stream_socket_server_v6;
pub(crate) use build_sockaddr_in6::emit_build_sockaddr_in6;
pub(crate) use opendir_glob::emit_opendir_glob;
pub(crate) use socket_scheme::emit_addr_is_udp;
pub(crate) use format_sockaddr::{
    emit_format_sockaddr_in, emit_format_sockaddr_in6, emit_format_sockaddr_unix,
};
pub(crate) use data_stream::emit_data_stream;
pub(crate) use stream_filter::emit_apply_stream_filter;
pub(crate) use fsockopen::emit_fsockopen;
pub(crate) use ftp::emit_ftp;
pub(crate) use http::emit_http;
pub(crate) use https::emit_https;
pub(crate) use stream_wrapper_register::emit_stream_wrapper_register;
pub(crate) use stream_wrapper_unregister::emit_stream_wrapper_unregister;
pub(crate) use http_build_request::emit_http_build_request;
pub(crate) use stream_context_get_ssl_peer_name::emit_get_ssl_peer_name;
pub(crate) use apply_socket_opts::{
    emit_apply_socket_bindto, emit_apply_socket_client_opts, emit_apply_socket_server_opts,
};
pub(crate) use stream_context_get_int_option::emit_get_int_context_option;
pub(crate) use stream_context_get_string_option::emit_get_string_context_option;
pub(crate) use stream_context_set_option_4::emit_stream_context_set_option_4;
pub(crate) use stream_copy_to_stream::emit_stream_copy_to_stream;
pub(crate) use stream_get_contents::emit_stream_get_contents;
pub(crate) use stream_get_line::emit_stream_get_line;
pub(crate) use stream_get_meta_data::emit_stream_get_meta_data;
pub(crate) use stream_socket_accept::emit_stream_socket_accept;
pub(crate) use stream_socket_client::emit_stream_socket_client;
pub(crate) use pclose::emit_pclose;
pub(crate) use popen::emit_popen;
pub(crate) use opendir::emit_opendir;
pub(crate) use readdir::emit_readdir;
pub(crate) use closedir::emit_closedir;
pub(crate) use rewinddir::emit_rewinddir;
pub(crate) use stream_socket_get_name::emit_stream_socket_get_name;
pub(crate) use stream_socket_pair::emit_stream_socket_pair;
pub(crate) use stream_socket_recvfrom::emit_stream_socket_recvfrom;
pub(crate) use stream_socket_sendto::emit_stream_socket_sendto;
pub(crate) use socket_backlog::emit_socket_backlog;
pub(crate) use stream_socket_server::emit_stream_socket_server;
pub(crate) use stream_socket_shutdown::emit_stream_socket_shutdown;
pub(crate) use unix_socket_client::emit_unix_socket_client;
pub(crate) use unix_socket_server::emit_unix_socket_server;
pub(crate) use stream_isatty::emit_stream_isatty;
pub(crate) use stream_select::emit_stream_select;
pub(crate) use stream_set_blocking::emit_stream_set_blocking;
pub(crate) use stream_set_timeout::emit_stream_set_timeout;
pub(crate) use streams_ext::emit_streams_ext;
pub(crate) use symlink::emit_symlink;
pub(crate) use tempnam::emit_tempnam;
pub(crate) use user_filter::{
    emit_apply_user_stream_filter, emit_resolve_user_filter_id,
    emit_stream_filter_attach_user, emit_stream_filter_register,
    emit_user_filter_release_fd,
};
pub(crate) use user_filter_brigade::emit_user_filter_brigade_invoke;
pub(crate) use user_wrapper::{
    emit_box_wrapper_stat_result, emit_user_wrapper_fclose, emit_user_wrapper_feof,
    emit_user_wrapper_fflush, emit_user_wrapper_flock, emit_user_wrapper_fread,
    emit_user_wrapper_fseek, emit_user_wrapper_fstat, emit_user_wrapper_ftell,
    emit_user_wrapper_ftruncate, emit_user_wrapper_fwrite,
};
pub(crate) use path_is_wrapper::emit_path_is_wrapper;
pub(crate) use readfile_wrapper::emit_readfile_wrapper;
pub(crate) use user_wrapper_cast::emit_user_wrapper_stream_cast;
pub(crate) use user_wrapper_dir::{
    emit_user_wrapper_dir_closedir, emit_user_wrapper_dir_readdir, emit_user_wrapper_dir_rewinddir,
    emit_user_wrapper_opendir,
};
pub(crate) use user_wrapper_path_op::{emit_user_wrapper_path_op, emit_user_wrapper_rename};
pub(crate) use stash_connect_host::emit_stash_connect_host;
pub(crate) use notification::emit_fire_notification;
pub(crate) use touch_meta_array::emit_touch_meta_array;
pub(crate) use user_wrapper_set_option::emit_user_wrapper_set_option;
pub(crate) use user_wrapper_url_stat::{
    emit_user_wrapper_url_stat, emit_user_wrapper_url_stat_field,
};
pub(crate) use var_dump_walk::{
    emit_var_dump_array_bool, emit_var_dump_array_float, emit_var_dump_array_int,
    emit_var_dump_array_mixed, emit_var_dump_array_str, emit_var_dump_emit_bool_line,
    emit_var_dump_emit_float_line, emit_var_dump_emit_indexed_key, emit_var_dump_emit_int_line,
    emit_var_dump_emit_null_line, emit_var_dump_emit_string_line,
};
