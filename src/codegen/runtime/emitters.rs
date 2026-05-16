//! Purpose:
//! Coordinates emission of all runtime helper labels for non-minimal targets.
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
use super::strings;
use super::system;
use super::x86_minimal::emit_runtime_linux_x86_64_minimal;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::{Arch, Platform};

pub(crate) fn emit_runtime(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_runtime_linux_x86_64_minimal(emitter);
        return;
    }

    emit_optional_linux_crypto_decls(emitter);
    diagnostics::emit_diagnostics(emitter);

    // String runtime functions
    strings::emit_itoa(emitter);
    strings::emit_resource_to_string(emitter);
    strings::emit_resource_write_stdout(emitter);
    strings::emit_ftoa(emitter);
    strings::emit_concat(emitter);
    strings::emit_atoi(emitter);
    strings::emit_str_eq(emitter);
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
    strings::emit_hex2bin(emitter);
    strings::emit_htmlspecialchars(emitter);
    strings::emit_html_entity_decode(emitter);
    strings::emit_urlencode(emitter);
    strings::emit_urldecode(emitter);
    strings::emit_rawurlencode(emitter);
    strings::emit_md5(emitter);
    strings::emit_sha1(emitter);
    strings::emit_hash(emitter);
    strings::emit_base64_encode(emitter);
    strings::emit_base64_decode(emitter);
    strings::emit_sprintf(emitter);
    strings::emit_sscanf(emitter);
    strings::emit_rtrim_mask(emitter);
    strings::emit_ltrim_mask(emitter);
    strings::emit_trim_mask(emitter);

    // Callable introspection runtime functions
    callables::emit_is_callable_runtime(emitter);

    // System runtime functions
    system::emit_build_argv(emitter);
    system::emit_time(emitter);
    system::emit_microtime(emitter);
    system::emit_php_uname(emitter);
    system::emit_getenv(emitter);
    system::emit_shell_exec(emitter);
    system::emit_date(emitter);
    system::emit_mktime(emitter);
    system::emit_strtotime(emitter);
    system::emit_json_encode_bool(emitter);
    system::emit_json_encode_null(emitter);
    system::emit_json_encode_str(emitter);
    system::emit_json_encode_mixed(emitter);
    system::emit_json_encode_float(emitter);
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
    system::emit_preg_strip(emitter);
    system::emit_pcre_to_posix(emitter);
    system::emit_preg_match(emitter);
    system::emit_preg_match_all(emitter);
    system::emit_preg_replace(emitter);
    system::emit_preg_split(emitter);
    system::emit_match_unhandled(emitter);
    system::emit_enum_from_fail(emitter);

    // Exception runtime functions
    exceptions::emit_exception_cleanup_frames(emitter);
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
    arrays::emit_array_union(emitter);
    arrays::emit_array_hash_union(emitter);
    arrays::emit_hash_array_union(emitter);
    arrays::emit_random_u32(emitter);
    arrays::emit_random_uniform(emitter);
    arrays::emit_sort_int(emitter, false);
    arrays::emit_sort_int(emitter, true);
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
    arrays::emit_hash_insert_owned(emitter);
    arrays::emit_hash_get(emitter);
    arrays::emit_hash_iter(emitter);
    arrays::emit_hash_union(emitter);
    arrays::emit_hash_count(emitter);
    arrays::emit_hash_free_deep(emitter);
    arrays::emit_array_key_exists(emitter);
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
    arrays::emit_array_fill_refcounted(emitter);
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
    arrays::emit_array_map_str(emitter);
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
    arrays::emit_mixed_strict_eq(emitter);
    arrays::emit_mixed_unbox(emitter);
    arrays::emit_mixed_write_stdout(emitter);
    arrays::emit_object_free_deep(emitter);
    arrays::emit_refcount(emitter);

    // Object runtime functions
    objects::emit_stdclass_new(emitter);
    objects::emit_stdclass_from_hash(emitter);
    objects::emit_stdclass_get(emitter);
    objects::emit_stdclass_set(emitter);
    objects::emit_mixed_property_get(emitter);
    objects::emit_mixed_property_set(emitter);
    objects::emit_mixed_array_get(emitter);
    objects::emit_json_encode_stdclass(emitter);

    // Buffer runtime functions
    buffers::emit_buffer_new(emitter);
    buffers::emit_buffer_len(emitter);
    buffers::emit_buffer_bounds_fail(emitter);
    buffers::emit_buffer_use_after_free(emitter);

    // I/O runtime functions
    io::emit_cstr(emitter);
    io::emit_fopen(emitter);
    io::emit_fgets(emitter);
    io::emit_feof(emitter);
    io::emit_fread(emitter);
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
    io::emit_fputcsv(emitter);
    io::emit_basename(emitter);
    io::emit_dirname(emitter);
    io::emit_dirname_levels(emitter);
    io::emit_fnmatch(emitter);
    io::emit_realpath(emitter);
    io::emit_pathinfo_str(emitter);
    io::emit_pathinfo_array(emitter);
    io::emit_modify(emitter);
    io::emit_streams_ext(emitter);
    io::emit_symlink(emitter);

    // Pointer runtime functions
    pointers::emit_ptoa(emitter);
    pointers::emit_ptr_check_nonnull(emitter);
    pointers::emit_str_to_cstr(emitter);
    pointers::emit_cstr_to_str(emitter);

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

fn emit_optional_linux_crypto_decls(emitter: &mut Emitter) {
    if emitter.target.platform == Platform::Linux {
        emitter.raw(".weak MD5");
        emitter.raw(".weak SHA1");
        emitter.raw(".weak SHA256");
        emitter.blank();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    #[test]
    fn test_linux_runtime_marks_crypto_symbols_weak() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::AArch64));
        emit_runtime(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains(".weak MD5\n"));
        assert!(asm.contains(".weak SHA1\n"));
        assert!(asm.contains(".weak SHA256\n"));
        assert!(!asm.contains("arc4random_uniform"));
    }

    #[test]
    fn test_aarch64_runtime_emits_fiber_routines() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_runtime(&mut emitter);
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
}
