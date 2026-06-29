//! Purpose:
//! Coordinates emission of all runtime helper labels for supported targets.
//! Orders strings, system helpers, exceptions, arrays, buffers, I/O, pointers, and fibers so dependencies are available.
//!
//! Called from:
//! - `crate::codegen::runtime::emit_runtime()`.
//!
//! Key details:
//! - Emission order is part of the runtime contract because helpers branch to labels and data symbols emitted elsewhere.

use super::arrays;
use super::buffers;
use super::callables;
use super::diagnostics;
use super::exceptions;
use super::fibers;
use super::generators;
use super::io;
use super::objects;
use super::pointers;
use super::spl;
use super::strings;
use super::system;
use crate::codegen::emit::Emitter;
use crate::codegen::RuntimeFeatures;

/// Emits all runtime helper labels in dependency order for supported targets.
///
/// Emits in order: diagnostics, strings, callables, system, exceptions, generators,
/// arrays, SPL, objects, buffers, I/O, pointers, fibers.
///
/// Each category is emitted before any code that depends on it, ensuring labels
/// are available when branches are assembled.
pub(crate) fn emit_runtime(emitter: &mut Emitter, features: RuntimeFeatures) {
    diagnostics::emit_diagnostics(emitter);

    // String runtime functions
    strings::emit_itoa(emitter);
    strings::emit_resource_to_string(emitter);
    strings::emit_resource_write_stdout(emitter);
    strings::emit_ftoa(emitter);
    strings::emit_concat(emitter);
    strings::emit_atoi(emitter);
    strings::emit_str_eq(emitter);
    strings::emit_str_to_number(emitter);
    strings::emit_str_to_int(emitter);
    strings::emit_str_loose_eq(emitter);
    strings::emit_number_format(emitter);
    strings::emit_strcopy(emitter);
    strings::emit_str_persist(emitter);
    strings::emit_strtolower(emitter);
    strings::emit_strtoupper(emitter);
    strings::emit_trim(emitter);
    strings::emit_ltrim(emitter);
    strings::emit_rtrim(emitter);
    strings::emit_strpos(emitter);
    strings::emit_strrpos(emitter);
    strings::emit_str_repeat(emitter);
    strings::emit_strrev(emitter);
    strings::emit_grapheme_strrev(emitter);
    strings::emit_chr(emitter);
    strings::emit_strcmp(emitter);
    strings::emit_strcasecmp(emitter);
    strings::emit_str_starts_with(emitter);
    strings::emit_str_ends_with(emitter);
    strings::emit_str_replace(emitter);
    strings::emit_explode(emitter);
    strings::emit_implode(emitter);
    strings::emit_implode_int(emitter);
    strings::emit_ucwords(emitter);
    strings::emit_str_ireplace(emitter);
    strings::emit_substr_replace(emitter);
    strings::emit_str_pad(emitter);
    strings::emit_str_split(emitter);
    strings::emit_addslashes(emitter);
    strings::emit_stripslashes(emitter);
    strings::emit_nl2br(emitter);
    strings::emit_wordwrap(emitter);
    strings::emit_bin2hex(emitter);
    strings::emit_long2ip(emitter);
    strings::emit_ip2long(emitter);
    strings::emit_inet_ntop(emitter);
    strings::emit_inet_pton(emitter);
    strings::emit_hex2bin(emitter);
    strings::emit_htmlspecialchars(emitter);
    strings::emit_html_entity_decode(emitter);
    strings::emit_urlencode(emitter);
    strings::emit_urldecode(emitter);
    strings::emit_rawurlencode(emitter);
    strings::emit_md5(emitter);
    strings::emit_sha1(emitter);
    strings::emit_crc32(emitter);
    strings::emit_hash(emitter);
    strings::emit_hash_hmac(emitter);
    strings::emit_hash_equals(emitter);
    strings::emit_hash_algos_list(emitter);
    strings::emit_hash_context(emitter);
    strings::emit_digest_to_string(emitter);
    strings::emit_base64_encode(emitter);
    strings::emit_base64_decode(emitter);
    strings::emit_sprintf(emitter);
    strings::emit_vsprintf(emitter);
    strings::emit_sscanf(emitter);
    strings::emit_rtrim_mask(emitter);
    strings::emit_ltrim_mask(emitter);
    strings::emit_trim_mask(emitter);

    // Callable introspection runtime functions
    callables::emit_is_callable_runtime(emitter);
    callables::emit_callable_descriptor_release(emitter);
    callables::emit_closure_bind(emitter);

    // System runtime functions
    system::emit_build_argv(emitter);
    system::emit_time(emitter);
    system::emit_microtime(emitter);
    system::emit_microtime_build_into(emitter);
    system::emit_microtime_str(emitter);
    system::emit_microtime_mixed(emitter);
    system::emit_php_uname(emitter);
    system::emit_getenv(emitter);
    system::emit_shell_exec(emitter);
    system::emit_date(emitter);
    system::emit_date_default_timezone(emitter);
    system::emit_checkdate(emitter);
    system::emit_getdate(emitter);
    system::emit_localtime(emitter);
    system::emit_hrtime(emitter);
    system::emit_mktime(emitter);
    system::emit_strtotime(emitter);
    system::emit_json_encode_bool(emitter);
    system::emit_json_encode_null(emitter);
    system::emit_json_encode_str(emitter);
    system::emit_json_encode_mixed(emitter);
    system::emit_json_encode_float(emitter);
    system::emit_json_ftoa(emitter);
    system::emit_json_encode_object(emitter);
    system::emit_json_pretty_helpers(emitter);
    system::emit_json_throw_error(emitter);
    system::emit_json_depth_enter(emitter);
    system::emit_json_depth_exit(emitter);
    system::emit_json_encode_array_dynamic(emitter);
    system::emit_json_encode_array_int(emitter);
    system::emit_json_encode_array_str(emitter);
    system::emit_json_encode_assoc(emitter);
    system::emit_json_decode(emitter);
    system::emit_json_decode_mixed(emitter);
    system::emit_json_last_error_msg(emitter);
    system::emit_json_validate(emitter);
    system::emit_serialize(emitter);
    system::emit_unserialize(emitter);
    if features.regex {
        system::emit_preg_strip(emitter);
        system::emit_pcre_to_posix(emitter);
        system::emit_preg_match(emitter);
        system::emit_preg_match_all(emitter);
        system::emit_preg_replace(emitter);
        system::emit_preg_replace_callback(emitter);
        system::emit_preg_split(emitter);
    }
    system::emit_match_unhandled(emitter);

    // Exception runtime functions
    exceptions::emit_exception_cleanup_frames(emitter);
    exceptions::emit_class_implements_interface(emitter);
    exceptions::emit_dynamic_instanceof(emitter);
    exceptions::emit_exception_matches(emitter);
    exceptions::emit_throw_current(emitter);
    exceptions::emit_rethrow_current(emitter);

    // Generator runtime helpers for Iterator methods, send/throw, and return-value retrieval.
    generators::emit_generator_runtime(emitter);

    // Array runtime functions
    arrays::emit_heap_alloc(emitter);
    arrays::emit_heap_debug_fail(emitter);
    arrays::emit_heap_debug_check_live(emitter);
    arrays::emit_heap_debug_validate_free_list(emitter);
    arrays::emit_heap_debug_report(emitter);
    arrays::emit_heap_kind(emitter);
    arrays::emit_heap_free(emitter);
    arrays::emit_array_free_deep(emitter);
    arrays::emit_array_clone_shallow(emitter);
    arrays::emit_array_ensure_unique(emitter);
    arrays::emit_array_grow(emitter);
    arrays::emit_array_new(emitter);
    arrays::emit_array_push_int(emitter);
    arrays::emit_array_push_refcounted(emitter);
    arrays::emit_array_push_str(emitter);
    arrays::emit_array_set_int(emitter);
    arrays::emit_array_set_mixed(emitter);
    arrays::emit_array_set_mixed_key(emitter);
    arrays::emit_array_set_refcounted(emitter);
    arrays::emit_array_set_str(emitter);
    arrays::emit_array_union(emitter);
    arrays::emit_array_hash_union(emitter);
    arrays::emit_hash_array_union(emitter);
    arrays::emit_random_u32(emitter);
    arrays::emit_random_uniform(emitter);
    arrays::emit_sort_int(emitter, false);
    arrays::emit_sort_int(emitter, true);
    arrays::emit_sort_str(emitter, false);
    arrays::emit_sort_str(emitter, true);
    arrays::emit_hash_fnv1a(emitter);
    arrays::emit_hash_key_hash(emitter);
    arrays::emit_hash_key_eq(emitter);
    arrays::emit_hash_normalize_key(emitter);
    arrays::emit_hash_clone_shallow(emitter);
    arrays::emit_hash_ensure_unique(emitter);
    arrays::emit_hash_new(emitter);
    arrays::emit_hash_grow(emitter);
    arrays::emit_hash_may_have_cyclic_values(emitter);
    arrays::emit_hash_set(emitter);
    arrays::emit_hash_unset(emitter);
    arrays::emit_hash_append(emitter);
    arrays::emit_hash_insert_owned(emitter);
    arrays::emit_hash_get(emitter);
    arrays::emit_hash_iter(emitter);
    arrays::emit_hash_union(emitter);
    arrays::emit_hash_to_mixed(emitter);
    arrays::emit_hash_count(emitter);
    arrays::emit_hash_free_deep(emitter);
    arrays::emit_array_key_exists(emitter);
    arrays::emit_undefined_array_key_warning(emitter);
    arrays::emit_array_search(emitter);
    arrays::emit_array_reverse(emitter);
    arrays::emit_array_reverse_refcounted(emitter);
    arrays::emit_array_sum(emitter);
    arrays::emit_array_product(emitter);
    arrays::emit_array_shift(emitter);
    arrays::emit_array_unshift(emitter);
    arrays::emit_array_merge(emitter);
    arrays::emit_array_merge_refcounted(emitter);
    arrays::emit_array_slice(emitter);
    arrays::emit_array_slice_refcounted(emitter);
    arrays::emit_range(emitter);
    arrays::emit_shuffle(emitter);
    arrays::emit_array_unique(emitter);
    arrays::emit_array_unique_refcounted(emitter);
    arrays::emit_array_rand(emitter);
    arrays::emit_array_fill(emitter);
    arrays::emit_array_fill_assoc(emitter);
    arrays::emit_array_fill_refcounted(emitter);
    arrays::emit_array_fill_str(emitter);
    arrays::emit_array_pad(emitter);
    arrays::emit_array_pad_refcounted(emitter);
    arrays::emit_array_diff(emitter);
    arrays::emit_array_diff_refcounted(emitter);
    arrays::emit_array_intersect(emitter);
    arrays::emit_array_intersect_refcounted(emitter);
    arrays::emit_array_flip(emitter);
    arrays::emit_array_flip_string(emitter);
    arrays::emit_array_combine(emitter);
    arrays::emit_array_combine_refcounted(emitter);
    arrays::emit_array_fill_keys(emitter);
    arrays::emit_array_fill_keys_refcounted(emitter);
    arrays::emit_array_chunk(emitter);
    arrays::emit_array_chunk_refcounted(emitter);
    arrays::emit_array_column(emitter);
    arrays::emit_array_column_mixed(emitter);
    arrays::emit_array_column_ref(emitter);
    arrays::emit_array_column_str(emitter);
    arrays::emit_array_splice(emitter);
    arrays::emit_array_splice_refcounted(emitter);
    arrays::emit_array_diff_key(emitter);
    arrays::emit_array_intersect_key(emitter);
    arrays::emit_asort(emitter);
    arrays::emit_ksort(emitter);
    arrays::emit_natsort(emitter);
    arrays::emit_array_map(emitter);
    arrays::emit_array_map_mixed(emitter);
    arrays::emit_array_map_str(emitter);
    arrays::emit_array_map_str_owned(emitter);
    arrays::emit_array_filter(emitter);
    arrays::emit_array_filter_refcounted(emitter);
    arrays::emit_array_reduce(emitter);
    arrays::emit_array_walk(emitter);
    arrays::emit_usort(emitter);
    arrays::emit_array_to_mixed(emitter);
    arrays::emit_array_merge_into(emitter);
    arrays::emit_array_merge_into_refcounted(emitter);
    arrays::emit_decref_any(emitter);
    arrays::emit_decref_mixed(emitter);
    arrays::emit_gc_note_child_ref(emitter);
    arrays::emit_gc_mark_reachable(emitter);
    arrays::emit_gc_collect_cycles(emitter);
    arrays::emit_mixed_from_value(emitter);
    arrays::emit_mixed_abs(emitter);
    arrays::emit_mixed_instanceof(emitter);
    arrays::emit_iterable_unsupported_kind(emitter);
    arrays::emit_iterable_write_stdout(emitter);
    arrays::emit_mixed_cast_bool(emitter);
    arrays::emit_mixed_cast_float(emitter);
    arrays::emit_mixed_cast_int(emitter);
    arrays::emit_mixed_cast_string(emitter);
    arrays::emit_mixed_count(emitter);
    arrays::emit_mixed_free_deep(emitter);
    arrays::emit_mixed_is_empty(emitter);
    arrays::emit_mixed_numeric_binops(emitter);
    arrays::emit_mixed_strict_eq(emitter);
    arrays::emit_mixed_unbox(emitter);
    arrays::emit_mixed_write_stdout(emitter);
    arrays::emit_object_free_deep(emitter);
    arrays::emit_refcount(emitter);

    // SPL runtime-managed containers
    spl::emit_doubly_linked_list_runtime(emitter);
    spl::emit_fixed_array_runtime(emitter);

    // Object runtime functions
    objects::emit_stdclass_new(emitter);
    objects::emit_stdclass_from_hash(emitter);
    objects::emit_stdclass_get(emitter);
    objects::emit_stdclass_set(emitter);
    objects::emit_mixed_property_get(emitter);
    objects::emit_mixed_property_set(emitter);
    objects::emit_mixed_array_get(emitter);
    objects::emit_mixed_array_set(emitter);
    objects::emit_new_by_name(emitter);
    objects::emit_call_object_destructor(emitter);
    objects::emit_json_encode_stdclass(emitter);

    // Buffer runtime functions
    buffers::emit_buffer_new(emitter);
    buffers::emit_buffer_len(emitter);
    buffers::emit_buffer_bounds_fail(emitter);
    buffers::emit_buffer_use_after_free(emitter);

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
    io::emit_ftp(emitter);
    io::emit_http(emitter);
    io::emit_https(emitter);
    io::emit_fsockopen(emitter);
    io::emit_stream_wrapper_register(emitter);
    io::emit_stream_wrapper_unregister(emitter);
    io::emit_stream_socket_server(emitter);
    io::emit_stream_socket_client(emitter);
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
    io::emit_apply_socket_client_opts(emitter);
    io::emit_apply_socket_server_opts(emitter);
    io::emit_socket_backlog(emitter);
    io::emit_apply_socket_bindto(emitter);
    io::emit_get_ssl_peer_name(emitter);
    io::emit_http_build_request(emitter);
    io::emit_fread(emitter);
    io::emit_fwrite(emitter);
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
    io::emit_user_wrapper_url_stat_field(emitter);
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
    io::emit_user_filter_release_fd(emitter);
    io::emit_var_dump_array_int(emitter);
    io::emit_var_dump_array_str(emitter);
    io::emit_var_dump_array_bool(emitter);
    io::emit_var_dump_array_float(emitter);
    io::emit_var_dump_array_mixed(emitter);
    io::emit_var_dump_emit_indexed_key(emitter);
    io::emit_var_dump_emit_string_key(emitter);
    io::emit_var_dump_hash(emitter);
    io::emit_var_dump_emit_int_line(emitter);
    io::emit_var_dump_emit_string_line(emitter);
    io::emit_var_dump_emit_bool_line(emitter);
    io::emit_var_dump_emit_float_line(emitter);
    io::emit_var_dump_emit_null_line(emitter);
    io::emit_print_r_spaces(emitter);
    io::emit_print_r_open(emitter);
    io::emit_print_r_close(emitter);
    io::emit_print_r_int_key(emitter);
    io::emit_print_r_str_key(emitter);
    io::emit_print_r_value(emitter);
    io::emit_print_r_indexed(emitter);
    io::emit_print_r_hash(emitter);
    io::emit_file_get_contents(emitter);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::{Arch, Platform, Target};

    /// Verifies that AArch64 runtime emits fiber routines.
    #[test]
    fn test_aarch64_runtime_emits_fiber_routines() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_runtime(&mut emitter, RuntimeFeatures::all());
        let asm = emitter.output();

        for sym in [
            "__rt_fiber_alloc_stack",
            "__rt_fiber_free_stack",
            "__rt_fiber_switch",
            "__rt_fiber_entry",
            "__rt_fiber_construct",
            "__rt_fiber_start",
            "__rt_fiber_resume",
            "__rt_fiber_suspend",
            "__rt_fiber_throw",
            "__rt_fiber_get_current",
            "__rt_fiber_get_return",
            "__rt_fiber_state_eq",
        ] {
            assert!(
                asm.contains(&format!(".globl {}\n", sym)),
                "fiber runtime missing global symbol {}",
                sym
            );
        }
    }

    /// Verifies optional regex helpers are omitted when the program does not reference them.
    #[test]
    fn test_runtime_can_omit_regex_helpers() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_runtime(&mut emitter, RuntimeFeatures::none());
        let asm = emitter.output();

        assert!(!asm.contains("__rt_preg_match:"));
        assert!(!asm.contains("__rt_preg_replace:"));
        assert!(!asm.contains("__rt_preg_split:"));
    }

    /// Verifies that Linux x86_64 uses the shared runtime surface.
    #[test]
    fn test_linux_x86_64_runtime_uses_shared_surface() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_runtime(&mut emitter, RuntimeFeatures::all());
        let asm = emitter.output();

        for sym in [
            "__rt_hash_count",
            "__rt_gc_note_child_ref",
            "__rt_incref",
            "__rt_decref_array",
            "__rt_json_encode_assoc",
            "__rt_preg_match",
            "__rt_fiber_alloc_stack",
        ] {
            assert!(
                asm.contains(&format!(".globl {}\n", sym)),
                "linux x86_64 shared runtime missing global symbol {}",
                sym
            );
        }
    }
}
