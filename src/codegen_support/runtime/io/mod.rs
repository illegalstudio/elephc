//! Purpose:
//! Collects file, directory, path, stat, CSV, glob, and descriptor runtime emitters.
//! The module owns re-export wiring for helpers that adapt PHP I/O builtins to libc and runtime arrays.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` during the I/O runtime section.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

/// Shortest scheme a `scheme://` path can name, and therefore the index every
/// wrapper-dispatch scan starts its `://` search at.
///
/// PHP requires `n > 1` in `php_stream_locate_url_wrapper`: a single-letter scheme
/// is a Windows drive letter, never a wrapper. Measured against reference PHP —
/// `stream_wrapper_register("f", "W")` returns true and `f` appears in
/// `stream_get_wrappers()`, but `f://x` never reaches the wrapper. Starting the scan
/// here is what enforces it: a `://` at index 0 or 1 is simply never found.
pub(crate) const MIN_WRAPPER_SCHEME_LEN: usize = 2;

mod basename;
mod cstr;
mod file_scheme;
mod path_op_warning;
mod disk_space;
mod dirname;
mod dirname_levels;
mod feof;
mod fgetcsv;
mod fgets;
mod file;
mod file_get_contents;
mod file_get_contents_range;
mod file_get_contents_url;
mod file_put_contents;
mod fd_write;
mod fnmatch;
mod fopen;
mod data_stream_dynamic;
mod php_filter_dynamic;
mod php_wrapper_open;
mod php_fd_open;
mod fputcsv;
mod fread;
mod stream_context_shape;
mod stream_select_cast_warning;
mod user_wrapper_seek_reconcile;
mod stream_select_memory_guard;
mod stream_pending;
mod filter_inert;
mod filter_create_warning;
mod filter_param_warning;
mod filter_params;
mod fsockopen_meta;
mod resource_id_burn;
mod fd_set_append;
mod fread_filtered;
mod fwrite;
mod http_response;
mod php_input;
mod stdout_write;
mod ob_buffer;
mod ob_handler;
mod ob_status;
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
mod principal_lookup;
mod pathinfo_array;
mod pathinfo_str;
mod realpath;
mod scandir;
mod stat;
mod stat_array;
mod stat_ext;
mod stat_mode_access;
mod socket_addr;
pub(crate) mod socket_errno;
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
mod builtin_filter_id;
mod iconv_spec_split;
mod builtin_wrapper_index;
mod stream_filter;
mod fsockopen;
mod ftp;
mod http;
mod http_open_url;
mod https;
mod notification;
mod stream_wrapper_register;
mod stream_wrapper_unregister;
mod http_build_request;
mod apply_socket_opts;
mod stream_context_get_int_option;
mod stream_context_get_string_option;
mod stream_context_get_usec_option;
mod stream_context_merge_options;
mod stream_context_set_option_4;
mod stream_copy_to_stream;
mod stream_get_contents;
mod stream_get_line;
mod stream_get_meta_data;
mod stream_get_filters;
mod stream_get_wrappers;
mod http_response_headers;
mod stream_record_meta;
mod socket_connect_warning;
mod socket_gai_message;
mod stream_record_mode;
mod stream_transport;
mod stream_is_local;
mod stream_supports_lock;
mod filter_missing_warning;
mod open_failed_warning;
mod wrapper_disabled_warning;
mod unknown_wrapper_warning;
mod errno_warning;
mod append_position;
mod stream_type_name;
mod stream_socket_accept;
mod stream_socket_client;
mod pclose;
mod popen;
mod opendir;
mod glob_dir_next;
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
mod copy_wrapper;
mod readfile_wrapper;
mod user_filter;
mod user_filter_brigade;
mod user_filter_close_flush;
mod stash_connect_host;
mod touch_meta_array;
mod user_wrapper;
mod user_wrapper_unbox;
mod user_wrapper_cast;
mod user_wrapper_dir;
mod user_wrapper_path_op;
mod user_wrapper_construct;
mod user_wrapper_set_option;
mod user_wrapper_table;
mod user_wrapper_url_stat;
mod user_wrapper_url_stat_fields;
mod print_r_walk;
mod print_r_buffer;
mod var_dump_object;
mod var_dump_walk;

pub(crate) use basename::emit_basename;
pub(crate) use cstr::emit_cstr;
pub(crate) use file_scheme::emit_path_cstr;
pub(crate) use file_scheme::emit_path_diag_name;
pub(crate) use path_op_warning::emit_path_op_warning;
pub(crate) use path_op_warning::emit_rename_warning;
pub(crate) use disk_space::emit_disk_space;
pub(crate) use dirname::emit_dirname;
pub(crate) use dirname_levels::emit_dirname_levels;
pub(crate) use feof::{emit_feof, emit_feof_call, emit_stream_eof_known};
pub(crate) use fgetcsv::emit_fgetcsv;
pub(crate) use fgets::emit_fgets;
pub(crate) use file::emit_file;
pub(crate) use file_get_contents::emit_file_get_contents;
pub(crate) use file_get_contents_range::{
    emit_file_get_contents_range, FILE_GET_CONTENTS_SEEK_MESSAGES,
};
pub(crate) use file_get_contents_url::emit_file_get_contents_url;
pub(crate) use fd_write::emit_fd_write;
pub(crate) use file_put_contents::emit_file_put_contents;
pub(crate) use fnmatch::emit_fnmatch;
pub(crate) use fopen::{
    emit_fopen, unable_to_locate_wrapper_message, CHMOD_NON_STANDARD_STREAM,
    FOPEN_WRAPPER_DISABLED_MESSAGE, NO_SUITABLE_WRAPPER_REASON,
};
pub(crate) use fputcsv::emit_fputcsv;
pub(crate) use fread::emit_fread;
pub(crate) use fread_filtered::emit_fread_filtered;
pub(crate) use fwrite::emit_fwrite;
pub(crate) use http_response::{emit_header, emit_http_response_code};
pub(crate) use http_open_url::emit_http_open_url;
pub(crate) use php_input::emit_php_input;
pub(crate) use stdout_write::emit_stdout_write;
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
pub(crate) use principal_lookup::emit_principal_lookup;
pub(crate) use pathinfo_array::emit_pathinfo_array;
pub(crate) use pathinfo_str::emit_pathinfo_str;
pub(crate) use realpath::emit_realpath;
pub(crate) use scandir::emit_scandir;
pub(crate) use stat::emit_stat;
pub(crate) use stat_array::emit_stat_array;
pub(crate) use stat_ext::emit_stat_ext;
pub(crate) use stat_mode_access::emit_stat_mode_access;
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
pub(crate) use builtin_filter_id::{emit_builtin_filter_id, emit_builtin_filter_table};
pub(crate) use iconv_spec_split::{emit_iconv_spec_split, ICONV_SPEC_BUFFER_BYTES};
pub(crate) use builtin_wrapper_index::{
    emit_builtin_wrapper_index, emit_builtin_wrapper_table, emit_stream_wrapper_restore_diag,
};
pub(crate) use stream_filter::{emit_apply_stream_filter, emit_stream_filter_default_mode};
pub(crate) use fsockopen::emit_fsockopen;
pub(crate) use ftp::emit_ftp;
pub(crate) use http::emit_http;
pub(crate) use https::emit_https;
pub(crate) use stream_wrapper_register::{
    emit_stream_wrapper_register, BAD_PROTOCOL_WARNING, DUPLICATE_PROTOCOL_WARNING,
};
pub(crate) use user_wrapper_table::{
    emit_load_flags_base, emit_load_handles_base, emit_load_handles_cap, emit_load_table_base,
    emit_load_table_cap,
    emit_user_wrapper_handles_reserve, emit_user_wrappers_reserve,
};
pub(crate) use stream_wrapper_unregister::emit_stream_wrapper_unregister;
pub(crate) use http_build_request::emit_http_build_request;
pub(crate) use apply_socket_opts::{
    emit_apply_socket_bindto, emit_apply_socket_client_opts, emit_apply_socket_server_opts,
};
pub(crate) use stream_context_get_int_option::emit_get_int_context_option;
pub(crate) use stream_context_get_string_option::emit_get_string_context_option;
pub(crate) use stream_context_get_usec_option::emit_get_usec_context_option;
pub(crate) use stream_context_merge_options::emit_stream_context_merge_options;
pub(crate) use stream_context_set_option_4::emit_stream_context_set_option_4;
pub(crate) use stream_copy_to_stream::emit_stream_copy_to_stream;
pub(crate) use stream_get_contents::emit_stream_get_contents;
pub(crate) use stream_get_line::emit_stream_get_line;
pub(crate) use stream_context_shape::emit_stream_context_options_shape_ok;
pub(crate) use stream_select_cast_warning::emit_stream_select_cast_warning;
pub(crate) use user_wrapper_seek_reconcile::emit_user_wrapper_seek_reconcile;
pub(crate) use user_wrapper_seek_reconcile::emit_user_wrapper_lacks_seek;
pub(crate) use stream_supports_lock::emit_stream_own_its_file;
pub(crate) use stream_supports_lock::emit_stream_unlink_if_owned;
pub(crate) use stream_select_memory_guard::emit_stream_select_memory_guard;
pub(crate) use stream_pending::{
    emit_stream_pending_clear, emit_stream_pending_consume, emit_stream_pending_fill,
    emit_stream_pending_append, emit_stream_pending_held, emit_stream_pending_put,
    emit_stream_pending_take,
    emit_stream_temp_eof_probe,
};
pub(crate) use filter_inert::emit_filter_mark_inert;
pub(crate) use filter_create_warning::emit_filter_create_warning;
pub(crate) use filter_param_warning::emit_filter_param_warning;
pub(crate) use filter_params::{emit_asf_params_load, emit_filter_absorb_params};
pub(crate) use fsockopen_meta::emit_stream_record_fsockopen_meta;
pub(crate) use resource_id_burn::emit_resource_id_burn;
pub(crate) use fd_set_append::emit_fd_set_append;
pub(crate) use stream_get_meta_data::emit_stream_get_meta_data;
pub(crate) use stream_get_filters::emit_stream_get_filters;
pub(crate) use stream_get_wrappers::emit_stream_get_wrappers;
pub(crate) use http_response_headers::{
    emit_get_http_response_headers, emit_http_clear_last_response_headers,
    emit_http_get_last_response_headers,
};
pub(crate) use stream_record_meta::emit_stream_record_meta;
pub(crate) use socket_connect_warning::{
    emit_socket_connect_warning, SOCKET_WARNING_CLIENT, SOCKET_WARNING_FSOCKOPEN,
    SOCKET_WARNING_SERVER,
};
pub(crate) use socket_gai_message::emit_gai_publish;
pub(crate) use stream_record_mode::emit_stream_record_mode;
pub(crate) use stream_transport::{emit_stream_record_transport, emit_stream_transport};
pub(crate) use stream_is_local::emit_stream_is_local_path;
pub(crate) use stream_supports_lock::emit_stream_supports_lock;
pub(crate) use filter_missing_warning::{emit_filter_missing_warning, FILTER_MISSING_MSG_CAPACITY};
pub(crate) use open_failed_warning::{
    emit_open_failed_warning, BAD_MODE_REASON_CAPACITY, BAD_MODE_TAIL, OPEN_FAILED_MIDDLE,
    OPEN_FAILED_MSG_CAPACITY, WRAPPER_REFUSAL_REASONS,
};
pub(crate) use wrapper_disabled_warning::{
    emit_wrapper_disabled_open_warning, NO_WRAPPER_DIRECTORY_TAIL, NO_WRAPPER_STREAM_TAIL,
    WARNING_HEAD, WRAPPER_DISABLED_TAIL,
};
#[allow(unused_imports)]
pub(crate) use open_failed_warning::{
    GLOB_NO_STREAM_OPEN, OPEN_OPERATION_FAILED, PEER_FINGERPRINT_MISMATCH_LINE, PHP_FD_FORM,
    PHP_INVALID_URL_LINE,
};
pub(crate) use unknown_wrapper_warning::{
    emit_unknown_wrapper_warning, UNKNOWN_WRAPPER_MSG_CAPACITY,
};
pub(crate) use errno_warning::emit_errno_warning;
pub(crate) use append_position::{
    emit_stream_append_skip, emit_stream_clear_append_skip, emit_stream_filtered_pos,
    emit_stream_filtered_pos_set, emit_stream_wrapper_pos,
    emit_dynamic_context_deprecation, emit_stream_wrapper_pos_advance,
    emit_stream_wrapper_pos_set, emit_wrapper_context_notice,
};
pub(crate) use stream_type_name::{emit_stream_type_name, WRAPPER_ID_ZIP};
pub(crate) use stream_socket_accept::emit_stream_socket_accept;
pub(crate) use stream_socket_client::emit_stream_socket_client;
pub(crate) use socket_errno::emit_socket_strerror;
pub(crate) use php_wrapper_open::emit_php_wrapper_open;
pub(crate) use php_fd_open::{
    emit_php_fd_open, PHP_FD_DUP_HEAD, PHP_FD_DUP_MIDDLE, PHP_FD_DUP_TAIL, PHP_FD_RANGE_HEAD,
};
pub(crate) use php_filter_dynamic::{
    PHP_FILTER_OPEN_DEPTH_MAX, PHP_FILTER_PENDING_FRAME_SLOTS, PHP_FILTER_PENDING_MAX,
    emit_php_filter_dynamic,
};
pub(crate) use data_stream_dynamic::emit_data_stream_dynamic;
pub(crate) use pclose::emit_pclose;
pub(crate) use popen::emit_popen;
pub(crate) use opendir::emit_opendir;
pub(crate) use glob_dir_next::{emit_glob_dir_next, emit_path_is_glob_url};
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
pub(crate) use user_filter_close_flush::emit_stream_write_chain_close_flush;
pub(crate) use user_wrapper::emit_uw_post_read_eof;
pub(crate) use user_wrapper::{
    emit_box_wrapper_stat_result, emit_user_wrapper_fclose, emit_user_wrapper_feof,
    emit_user_wrapper_fflush, emit_user_wrapper_flock, emit_user_wrapper_fread,
    emit_user_wrapper_fseek, emit_user_wrapper_fstat, emit_user_wrapper_ftell,
    emit_user_wrapper_ftruncate, emit_user_wrapper_fwrite, emit_wrapper_missing_hook_warning,
};
pub(crate) use user_wrapper_unbox::emit_wrapper_unbox_int;
pub(crate) use path_is_wrapper::emit_path_is_wrapper;
pub(crate) use copy_wrapper::emit_copy_wrapper;
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
pub(crate) use user_wrapper_construct::emit_user_wrapper_construct;
pub(crate) use user_wrapper_set_option::emit_user_wrapper_set_option;
pub(crate) use user_wrapper_url_stat::{
    emit_clear_stat_cache, emit_user_wrapper_url_stat, emit_user_wrapper_url_stat_field,
};
pub(crate) use user_wrapper_url_stat_fields::{
    emit_user_wrapper_url_stat_readers, STAT_FIELD_ATIME, STAT_FIELD_CTIME, STAT_FIELD_GID,
    STAT_FIELD_INO, STAT_FIELD_MODE, STAT_FIELD_MTIME, STAT_FIELD_SIZE, STAT_FIELD_UID,
};
pub(crate) use print_r_walk::{
    emit_print_r_close, emit_print_r_hash, emit_print_r_indexed, emit_print_r_int_key,
    emit_print_r_open, emit_print_r_spaces, emit_print_r_str_key, emit_print_r_value,
};
pub(crate) use print_r_buffer::{emit_pr_append, emit_pr_finish, emit_pr_write};
pub(crate) use ob_buffer::{
    emit_ob_append, emit_ob_contents, emit_ob_flush_all, emit_ob_gated_ops, emit_ob_get_pop_ops,
    emit_ob_pop_free, emit_ob_process_and_write, emit_ob_queries, emit_ob_start,
};
pub(crate) use ob_handler::{
    emit_ob_apply_handler, emit_ob_eval_trampoline, emit_ob_invoke_descriptor,
    emit_ob_notice_named, emit_ob_result_to_bytes,
};
pub(crate) use ob_status::{emit_ob_get_status, emit_ob_list_handlers, emit_ob_status_entry};
pub(crate) use var_dump_object::{
    emit_var_dump_emit_object_key, emit_var_dump_emit_recursion_line,
    emit_var_dump_emit_uninit_line, emit_var_dump_object, emit_var_dump_open_object,
    emit_vd_obj_count, emit_vd_obj_desc, emit_vd_seen_find, emit_vd_seen_pop, emit_vd_seen_push,
};
pub(crate) use var_dump_walk::{
    emit_var_dump_array_bool, emit_var_dump_array_float, emit_var_dump_array_int,
    emit_var_dump_array_str, emit_var_dump_close_container, emit_var_dump_emit_bool_line,
    emit_var_dump_emit_float_line, emit_var_dump_emit_indexed_key, emit_var_dump_emit_int_line,
    emit_var_dump_emit_null_line, emit_var_dump_emit_resource_line, emit_var_dump_emit_string_key,
    emit_var_dump_emit_string_line,
    emit_var_dump_hash, emit_var_dump_indent_step, emit_var_dump_indexed,
    emit_var_dump_open_container, emit_var_dump_pad, emit_var_dump_value, emit_var_dump_write,
};
