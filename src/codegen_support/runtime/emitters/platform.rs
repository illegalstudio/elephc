//! Purpose:
//! Emits platform-facing runtime helpers for I/O, pointers, zvals, and fibers.
//!
//! Called from:
//! - `super::emit_runtime()` after managed runtime helpers.
//!
//! Key details:
//! - Preserves the dependency order among stream, output-buffering, pointer, zval, and fiber helpers.

use super::super::{fibers, io, pdo, pointers, zval};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::RuntimeFeatures;

/// Emits platform-facing runtime helpers and bridge primitives.
pub(super) fn emit_platform_runtime(emitter: &mut Emitter, features: RuntimeFeatures) {
    // I/O runtime functions
    // The terminal-stdout indirection every echo/print travels through. Always
    // emitted (every program can echo); its body differs for `--web` builds.
    io::emit_stdout_write(emitter, features.web);
    // Backs file_get_contents('php://input'); reads the request body under --web,
    // returns false (null) otherwise. Always emitted so the EIR call resolves.
    io::emit_php_input(emitter, features.web);
    // Back http_response_code()/header(); call the bridge setters under --web,
    // no-ops otherwise. Always emitted so the EIR calls resolve.
    io::emit_http_response_code(emitter, features.web);
    io::emit_header(emitter, features.web);
    io::emit_cstr(emitter);
    io::emit_disk_space(emitter);
    io::emit_fopen(emitter);
    io::emit_fgets(emitter);
    io::emit_feof(emitter);
    io::emit_stream_isatty(emitter);
    io::emit_stream_select(emitter);
    io::emit_stream_set_blocking(emitter);
    io::emit_stream_set_timeout(emitter);
    io::emit_stream_get_contents(emitter);
    io::emit_stream_get_line(emitter);
    io::emit_addr_is_udp(emitter);
    io::emit_resolve_host(emitter);
    io::emit_resolve_host_v6(emitter);
    io::emit_inet6_pton(emitter);
    io::emit_stream_socket_client_v6(emitter);
    io::emit_stream_socket_server_v6(emitter);
    io::emit_build_sockaddr_in6(emitter);
    io::emit_opendir_glob(emitter);
    io::emit_inet_addr_parse(emitter);
    io::emit_format_sockaddr_in(emitter);
    io::emit_format_sockaddr_in6(emitter);
    io::emit_format_sockaddr_unix(emitter);
    io::emit_data_stream(emitter);
    io::emit_apply_stream_filter(emitter);
    io::emit_stream_filter_default_mode(emitter);
    io::emit_ftp(emitter);
    io::emit_http(emitter);
    io::emit_https(emitter);
    io::emit_fsockopen(emitter);
    io::emit_stream_wrapper_register(emitter);
    io::emit_stream_wrapper_unregister(emitter);
    io::emit_stream_socket_server(emitter);
    io::emit_stream_socket_client(emitter);
    io::emit_socket_strerror(emitter);
    io::emit_unix_socket_server(emitter);
    io::emit_unix_socket_client(emitter);
    io::emit_stream_socket_accept(emitter);
    io::emit_stream_socket_shutdown(emitter);
    io::emit_stream_socket_sendto(emitter);
    io::emit_stream_socket_recvfrom(emitter);
    io::emit_stream_socket_get_name(emitter);
    io::emit_stream_socket_pair(emitter);
    io::emit_popen(emitter);
    io::emit_pclose(emitter);
    io::emit_opendir(emitter);
    io::emit_readdir(emitter);
    io::emit_closedir(emitter);
    io::emit_rewinddir(emitter);
    io::emit_fd_set_append(emitter);
    io::emit_resource_id_burn(emitter);
    io::emit_stream_record_fsockopen_meta(emitter);
    io::emit_filter_absorb_params(emitter);
    io::emit_asf_params_load(emitter);
    io::emit_filter_param_warning(emitter);
    io::emit_filter_create_warning(emitter);
    io::emit_filter_mark_inert(emitter);
    io::emit_stream_pending_put(emitter);
    io::emit_stream_pending_take(emitter);
    io::emit_stream_select_cast_warning(emitter);
    io::emit_stream_select_memory_guard(emitter);
    io::emit_stream_context_options_shape_ok(emitter);
    io::emit_stream_get_meta_data(emitter);
    io::emit_gethostname(emitter);
    io::emit_gethostbyname(emitter);
    io::emit_gethostbyaddr(emitter);
    io::emit_protoent_load(emitter);
    io::emit_getprotobyname(emitter);
    io::emit_getprotobynumber(emitter);
    io::emit_servent_load(emitter);
    io::emit_getservbyname(emitter);
    io::emit_getservbyport(emitter);
    io::emit_stream_copy_to_stream(emitter);
    io::emit_stream_context_set_option_4(emitter);
    io::emit_get_string_context_option(emitter);
    io::emit_get_int_context_option(emitter);
    io::emit_get_usec_context_option(emitter);
    io::emit_apply_socket_client_opts(emitter);
    io::emit_apply_socket_server_opts(emitter);
    io::emit_socket_backlog(emitter);
    io::emit_apply_socket_bindto(emitter);
    io::emit_http_build_request(emitter);
    io::emit_fread(emitter);
    io::emit_fread_filtered(emitter);
    io::emit_php_wrapper_open(emitter);
    io::emit_php_fd_open(emitter);
    io::emit_php_filter_dynamic(emitter);
    io::emit_data_stream_dynamic(emitter);
    io::emit_fwrite(emitter);
    io::emit_wrapper_missing_hook_warning(emitter);
    io::emit_wrapper_unbox_int(emitter);
    io::emit_user_wrapper_fclose(emitter);
    io::emit_user_wrapper_fread(emitter);
    io::emit_user_wrapper_fwrite(emitter);
    io::emit_user_wrapper_feof(emitter);
    io::emit_user_wrapper_flock(emitter);
    io::emit_user_wrapper_fseek(emitter);
    io::emit_user_wrapper_ftell(emitter);
    io::emit_user_wrapper_ftruncate(emitter);
    io::emit_user_wrapper_fflush(emitter);
    io::emit_box_wrapper_stat_result(emitter);
    io::emit_user_wrapper_fstat(emitter);
    io::emit_user_wrapper_url_stat(emitter);
    io::emit_user_wrapper_url_stat_readers(emitter);
    io::emit_user_wrapper_url_stat_field(emitter);
    io::emit_stat_mode_access(emitter);
    io::emit_path_is_wrapper(emitter);
    io::emit_readfile_wrapper(emitter);
    io::emit_user_wrapper_path_op(emitter);
    io::emit_user_wrapper_rename(emitter);
    io::emit_user_wrapper_set_option(emitter);
    io::emit_user_wrapper_opendir(emitter);
    io::emit_user_wrapper_dir_readdir(emitter);
    io::emit_user_wrapper_dir_closedir(emitter);
    io::emit_user_wrapper_dir_rewinddir(emitter);
    io::emit_touch_meta_array(emitter);
    io::emit_stash_connect_host(emitter);
    io::emit_fire_notification(emitter);
    io::emit_user_wrapper_stream_cast(emitter);
    io::emit_stream_filter_register(emitter);
    io::emit_resolve_user_filter_id(emitter);
    io::emit_stream_filter_attach_user(emitter);
    io::emit_apply_user_stream_filter(emitter);
    io::emit_user_filter_brigade_invoke(emitter);
    io::emit_stream_write_chain_close_flush(emitter);
    io::emit_user_filter_release_fd(emitter);
    io::emit_var_dump_array_int(emitter);
    io::emit_var_dump_array_str(emitter);
    io::emit_var_dump_array_bool(emitter);
    io::emit_var_dump_array_float(emitter);
    io::emit_var_dump_indexed(emitter);
    io::emit_var_dump_value(emitter);
    io::emit_var_dump_open_container(emitter);
    io::emit_var_dump_close_container(emitter);
    io::emit_var_dump_open_object(emitter);
    io::emit_var_dump_object(emitter);
    io::emit_var_dump_emit_object_key(emitter);
    io::emit_var_dump_emit_uninit_line(emitter);
    io::emit_var_dump_emit_recursion_line(emitter);
    io::emit_vd_obj_desc(emitter);
    io::emit_vd_obj_count(emitter);
    io::emit_vd_seen_find(emitter);
    io::emit_vd_seen_push(emitter);
    io::emit_vd_seen_pop(emitter);
    io::emit_var_dump_pad(emitter);
    io::emit_var_dump_indent_step(emitter);
    io::emit_var_dump_emit_indexed_key(emitter);
    io::emit_var_dump_emit_string_key(emitter);
    io::emit_var_dump_hash(emitter);
    io::emit_var_dump_emit_int_line(emitter);
    io::emit_var_dump_emit_string_line(emitter);
    io::emit_var_dump_emit_bool_line(emitter);
    io::emit_var_dump_emit_float_line(emitter);
    io::emit_var_dump_emit_null_line(emitter);
    io::emit_var_dump_emit_resource_line(emitter);
    io::emit_print_r_spaces(emitter);
    io::emit_print_r_open(emitter);
    io::emit_print_r_close(emitter);
    io::emit_print_r_int_key(emitter);
    io::emit_print_r_str_key(emitter);
    io::emit_print_r_value(emitter);
    io::emit_print_r_indexed(emitter);
    io::emit_print_r_hash(emitter);
    io::emit_pr_append(emitter);
    io::emit_pr_write(emitter);
    io::emit_pr_finish(emitter);
    // Output-buffering (ob_*) stack helpers. Always emitted: __rt_stdout_write,
    // __rt_pr_write, and the process-exit paths reference them unconditionally.
    io::emit_var_dump_write(emitter);
    io::emit_ob_start(emitter);
    io::emit_ob_append(emitter);
    io::emit_ob_contents(emitter);
    io::emit_ob_queries(emitter);
    io::emit_ob_process_and_write(emitter);
    io::emit_ob_pop_free(emitter);
    io::emit_ob_gated_ops(emitter);
    io::emit_ob_get_pop_ops(emitter);
    io::emit_ob_flush_all(emitter);
    io::emit_ob_apply_handler(emitter);
    io::emit_ob_result_to_bytes(emitter);
    io::emit_ob_invoke_descriptor(emitter);
    io::emit_ob_eval_trampoline(emitter);
    io::emit_ob_notice_named(emitter);
    io::emit_ob_status_entry(emitter);
    io::emit_ob_get_status(emitter);
    io::emit_ob_list_handlers(emitter);
    io::emit_file_get_contents(emitter);
    io::emit_file_get_contents_range(emitter);
    io::emit_file_put_contents(emitter);
    io::emit_file(emitter);
    io::emit_stat(emitter);
    io::emit_stat_ext(emitter);
    io::emit_stat_array(emitter);
    io::emit_fs(emitter);
    io::emit_getcwd(emitter);
    io::emit_scandir(emitter);
    io::emit_glob(emitter);
    io::emit_tempnam(emitter);
    io::emit_fgetcsv(emitter);
    io::emit_fd_write(emitter);
    io::emit_phar_write(emitter);
    io::emit_phar_read(emitter);
    io::emit_file_get_contents_url(emitter);
    io::emit_fputcsv(emitter);
    io::emit_basename(emitter);
    io::emit_dirname(emitter);
    io::emit_dirname_levels(emitter);
    io::emit_fnmatch(emitter);
    io::emit_realpath(emitter);
    io::emit_pathinfo_str(emitter);
    io::emit_pathinfo_array(emitter);
    io::emit_principal_lookup(emitter);
    io::emit_modify(emitter);
    io::emit_streams_ext(emitter);
    io::emit_symlink(emitter);

    // Pointer runtime functions
    pointers::emit_ptoa(emitter);
    pointers::emit_ptr_check_nonnull(emitter);
    pointers::emit_str_to_cstr(emitter);
    pointers::emit_cstr_to_str(emitter);
    pointers::emit_ptr_read_string(emitter);
    pointers::emit_ptr_write_string(emitter);

    // zval pack/unpack bridge runtime functions
    zval::emit_zval_string_new(emitter);
    zval::emit_zval_djbx33a(emitter);
    zval::emit_zval_pack(emitter);
    zval::emit_zval_pack_array_packed(emitter);
    zval::emit_zval_pack_array_hash(emitter);
    zval::emit_zval_unpack(emitter);
    zval::emit_zval_unpack_array(emitter);
    zval::emit_zval_type(emitter);
    zval::emit_zval_free_array(emitter);
    zval::emit_zval_free(emitter);

    // PDO Tier-D callback adapters. Emitted only when a PDO callback registration is reachable;
    // placed after arrays/heap/mixed and zval so adapters can call their shared helpers.
    if features.pdo_udf {
        pdo::emit_pdo_call_collation(emitter);
        pdo::emit_pdo_call_scalar(emitter);
        pdo::emit_pdo_call_agg_step(emitter);
        pdo::emit_pdo_call_agg_final(emitter);
    }

    // Fiber runtime functions (cooperative coroutines)
    fibers::emit_fiber_alloc_stack(emitter);
    fibers::emit_fiber_free_stack(emitter);
    fibers::emit_fiber_switch(emitter);
    fibers::emit_fiber_entry(emitter);
    fibers::emit_fiber_construct(emitter);
    fibers::emit_fiber_throw_state_error(emitter);
    fibers::emit_fiber_start(emitter);
    fibers::emit_fiber_resume(emitter);
    fibers::emit_fiber_suspend(emitter);
    fibers::emit_fiber_throw(emitter);
    fibers::emit_fiber_get_current(emitter);
    fibers::emit_fiber_get_return(emitter);
    fibers::emit_fiber_state_getter(emitter);
}
