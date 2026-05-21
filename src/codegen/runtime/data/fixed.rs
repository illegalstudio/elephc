//! Purpose:
//! Builds the cacheable fixed runtime data section as assembly text.
//! This owns heap globals, shared scratch buffers, fatal messages, lookup tables, and fixed runtime state.
//!
//! Called from:
//! - `crate::codegen::runtime::data::emit_runtime_data_fixed()`.
//!
//! Key details:
//! - Fixed symbols are cached across compilations, so only target-independent runtime data belongs here.

use super::{
    DIRNAME_LEVELS_MSG, PHP_UNAME_MODE_LEN_MSG, PHP_UNAME_MODE_VALUE_MSG,
    STR_REPEAT_TIMES_MSG,
};
use super::super::system;
use crate::types::checker::builtins::supported_builtin_function_names;

/// Emit the fixed runtime data section — cacheable across compilations.
/// Contains heap buffers, error messages, lookup tables, and other
/// data that does not depend on the user's program.
pub(crate) fn emit_runtime_data_fixed(heap_size: usize) -> String {
    let mut out = String::new();
    out.push_str(".data\n");
    out.push_str(".comm _concat_buf, 65536, 3\n");
    out.push_str(".comm _concat_off, 8, 3\n");
    out.push_str(".comm _global_argc, 8, 3\n");
    out.push_str(".comm _global_argv, 8, 3\n");
    out.push_str(".comm _exc_handler_top, 8, 3\n");
    out.push_str(".comm _exc_call_frame_top, 8, 3\n");
    out.push_str(".comm _exc_value, 8, 3\n");
    out.push_str(".comm _fiber_current, 8, 3\n");
    out.push_str(".comm _fiber_main_saved_sp, 8, 3\n");
    out.push_str(".comm _fiber_main_saved_exc, 8, 3\n");
    out.push_str(".comm _fiber_main_saved_call_frame, 8, 3\n");
    out.push_str(".comm _rt_diag_suppression, 8, 3\n");
    out.push_str(&format!(".comm _heap_buf, {}, 3\n", heap_size));
    out.push_str(".comm _heap_off, 8, 3\n");
    out.push_str(".comm _heap_free_list, 8, 3\n");
    out.push_str(".comm _heap_small_bins, 32, 3\n");
    out.push_str(".comm _heap_debug_enabled, 8, 3\n");
    out.push_str(".comm _gc_collecting, 8, 3\n");
    out.push_str(".comm _gc_release_suppressed, 8, 3\n");
    out.push_str(".comm _json_last_error, 8, 3\n");
    out.push_str(".comm _json_active_flags, 8, 3\n");
    out.push_str(".comm _json_active_depth, 8, 3\n");
    out.push_str(".comm _json_indent_depth, 8, 3\n");
    out.push_str(".comm _json_depth_limit, 8, 3\n");
    out.push_str(".comm _json_validate_idx, 8, 3\n");
    out.push_str(".comm _json_validate_ptr, 8, 3\n");
    out.push_str(".comm _json_validate_len, 8, 3\n");
    out.push_str(".comm _json_decode_assoc, 8, 3\n");
    out.push_str(&format!(".globl _heap_max\n_heap_max:\n    .quad {}\n", heap_size));
    out.push_str(".globl _heap_err_msg\n_heap_err_msg:\n    .ascii \"Fatal error: heap memory exhausted\\n\"\n");
    out.push_str(".globl _heap_dbg_bad_refcount_msg\n_heap_dbg_bad_refcount_msg:\n    .ascii \"Fatal error: heap debug detected bad refcount\\n\"\n");
    out.push_str(".globl _heap_dbg_double_free_msg\n_heap_dbg_double_free_msg:\n    .ascii \"Fatal error: heap debug detected double free\\n\"\n");
    out.push_str(".globl _heap_dbg_free_list_msg\n_heap_dbg_free_list_msg:\n    .ascii \"Fatal error: heap debug detected free-list corruption\\n\"\n");
    out.push_str(".globl _arr_cap_err_msg\n_arr_cap_err_msg:\n    .ascii \"Fatal error: array capacity exceeded\\n\"\n");
    out.push_str(".globl _buffer_bounds_msg\n_buffer_bounds_msg:\n    .ascii \"Fatal error: buffer index out of bounds\\n\"\n");
    out.push_str(".globl _buffer_uaf_msg\n_buffer_uaf_msg:\n    .ascii \"Fatal error: use of buffer after buffer_free()\\n\"\n");
    out.push_str(".globl _iterable_unsupported_kind_msg\n_iterable_unsupported_kind_msg:\n    .ascii \"Fatal error: foreach over iterable with unsupported kind\\n\"\n");
    out.push_str(".globl _iterable_array_str\n_iterable_array_str:\n    .ascii \"Array\"\n");
    out.push_str(".globl _match_unhandled_msg\n_match_unhandled_msg:\n    .ascii \"Fatal error: unhandled match case\\n\"\n");
    out.push_str(".globl _enum_from_msg\n_enum_from_msg:\n    .ascii \"Fatal error: enum case not found\\n\"\n");
    out.push_str(".globl _static_prop_private_access_msg\n_static_prop_private_access_msg:\n    .ascii \"Fatal error: Cannot access private static property\\n\"\n");
    out.push_str(".globl _ptr_null_err_msg\n_ptr_null_err_msg:\n    .ascii \"Fatal error: null pointer dereference\\n\"\n");
    out.push_str(".globl _ptr_read_string_len_err_msg\n_ptr_read_string_len_err_msg:\n    .ascii \"Fatal error: ptr_read_string() length must be non-negative\\n\"\n");
    out.push_str(&format!(
        ".globl _str_repeat_times_msg\n_str_repeat_times_msg:\n    .ascii {:?}\n",
        STR_REPEAT_TIMES_MSG
    ));
    out.push_str(".globl _uncaught_exc_msg\n_uncaught_exc_msg:\n    .ascii \"Fatal error: uncaught exception\\n\"\n");
    out.push_str(".globl _instanceof_target_type_msg\n_instanceof_target_type_msg:\n    .ascii \"Fatal error: Class name must be a valid object or a string\\n\"\n");
    out.push_str(".globl _diag_file_get_contents_failed_msg\n_diag_file_get_contents_failed_msg:\n    .ascii \"Warning: file_get_contents(): Failed to open stream\\n\"\n");
    out.push_str(".globl _diag_fopen_failed_msg\n_diag_fopen_failed_msg:\n    .ascii \"Warning: fopen(): Failed to open stream\\n\"\n");
    out.push_str(".globl _diag_define_already_defined_msg\n_diag_define_already_defined_msg:\n    .ascii \"Warning: define(): Constant already defined\\n\"\n");
    out.push_str(".globl _fiber_msg_already_started\n_fiber_msg_already_started:\n    .ascii \"Cannot start a fiber that has already been started\"\n");
    out.push_str(".globl _fiber_msg_not_suspended\n_fiber_msg_not_suspended:\n    .ascii \"Cannot resume a fiber that is not suspended\"\n");
    out.push_str(".globl _fiber_msg_throw_not_suspended\n_fiber_msg_throw_not_suspended:\n    .ascii \"Cannot resume a fiber that is not suspended\"\n");
    out.push_str(".globl _fiber_msg_not_terminated\n_fiber_msg_not_terminated:\n    .ascii \"Cannot get fiber return value: The fiber has not returned\"\n");
    out.push_str(".globl _fiber_msg_suspend_outside\n_fiber_msg_suspend_outside:\n    .ascii \"Cannot suspend outside of a fiber\"\n");
    out.push_str(".globl _fiber_msg_unsupported_callable\n_fiber_msg_unsupported_callable:\n    .ascii \"Fiber callable is not supported by this compiler\"\n");
    out.push_str(".globl _fiber_msg_stack_alloc_failed\n_fiber_msg_stack_alloc_failed:\n    .ascii \"Cannot allocate fiber stack\"\n");
    out.push_str(&emit_builtin_callable_data());
    out.push_str(".comm _gc_allocs, 8, 3\n");
    out.push_str(".comm _gc_frees, 8, 3\n");
    out.push_str(".comm _gc_live, 8, 3\n");
    out.push_str(".comm _gc_peak, 8, 3\n");
    out.push_str(".comm _cstr_buf, 4096, 3\n");
    out.push_str(".comm _cstr_buf2, 4096, 3\n");
    out.push_str(".comm _eof_flags, 256, 3\n");
    out.push_str(&emit_spl_autoload_extensions_data());
    out.push_str(".globl _heap_dbg_stats_prefix\n_heap_dbg_stats_prefix:\n    .ascii \"HEAP DEBUG: allocs=\"\n");
    out.push_str(".globl _heap_dbg_frees_label\n_heap_dbg_frees_label:\n    .ascii \" frees=\"\n");
    out.push_str(".globl _heap_dbg_live_blocks_label\n_heap_dbg_live_blocks_label:\n    .ascii \" live_blocks=\"\n");
    out.push_str(".globl _heap_dbg_live_bytes_label\n_heap_dbg_live_bytes_label:\n    .ascii \" live_bytes=\"\n");
    out.push_str(".globl _heap_dbg_peak_label\n_heap_dbg_peak_label:\n    .ascii \" peak_live_bytes=\"\n");
    out.push_str(".globl _heap_dbg_leak_prefix\n_heap_dbg_leak_prefix:\n    .ascii \"HEAP DEBUG: leak summary: \"\n");
    out.push_str(".globl _heap_dbg_live_blocks_short_label\n_heap_dbg_live_blocks_short_label:\n    .ascii \"live_blocks=\"\n");
    out.push_str(".globl _heap_dbg_clean_label\n_heap_dbg_clean_label:\n    .ascii \"clean\\n\"\n");
    out.push_str(".globl _heap_dbg_newline\n_heap_dbg_newline:\n    .ascii \"\\n\"\n");
    out.push_str(".globl _resource_id_prefix\n_resource_id_prefix:\n    .ascii \"Resource id #\"\n");
    out.push_str(".globl _fmt_g\n_fmt_g:\n    .asciz \"%.14G\"\n");
    out.push_str(".globl _b64_encode_tbl\n_b64_encode_tbl:\n    .ascii \"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/\"\n");
    out.push_str(".globl _b64_decode_tbl\n_b64_decode_tbl:\n");

    let mut decode_tbl = vec![0u8; 256];
    for (i, &c) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .iter()
        .enumerate()
    {
        decode_tbl[c as usize] = i as u8;
    }

    out.push_str("    .byte ");
    for (i, val) in decode_tbl.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&val.to_string());
    }
    out.push('\n');

    out.push_str(".globl _filetype_file\n_filetype_file:\n    .ascii \"file\"\n");
    out.push_str(".globl _filetype_dir\n_filetype_dir:\n    .ascii \"dir\"\n");
    out.push_str(".globl _filetype_link\n_filetype_link:\n    .ascii \"link\"\n");
    out.push_str(".globl _filetype_char\n_filetype_char:\n    .ascii \"char\"\n");
    out.push_str(".globl _filetype_block\n_filetype_block:\n    .ascii \"block\"\n");
    out.push_str(".globl _filetype_fifo\n_filetype_fifo:\n    .ascii \"fifo\"\n");
    out.push_str(".globl _filetype_socket\n_filetype_socket:\n    .ascii \"socket\"\n");
    out.push_str(".globl _filetype_unknown\n_filetype_unknown:\n    .ascii \"unknown\"\n");
    out.push_str(".globl _stat_key_dev\n_stat_key_dev:\n    .ascii \"dev\"\n");
    out.push_str(".globl _stat_key_ino\n_stat_key_ino:\n    .ascii \"ino\"\n");
    out.push_str(".globl _stat_key_mode\n_stat_key_mode:\n    .ascii \"mode\"\n");
    out.push_str(".globl _stat_key_nlink\n_stat_key_nlink:\n    .ascii \"nlink\"\n");
    out.push_str(".globl _stat_key_uid\n_stat_key_uid:\n    .ascii \"uid\"\n");
    out.push_str(".globl _stat_key_gid\n_stat_key_gid:\n    .ascii \"gid\"\n");
    out.push_str(".globl _stat_key_rdev\n_stat_key_rdev:\n    .ascii \"rdev\"\n");
    out.push_str(".globl _stat_key_size\n_stat_key_size:\n    .ascii \"size\"\n");
    out.push_str(".globl _stat_key_atime\n_stat_key_atime:\n    .ascii \"atime\"\n");
    out.push_str(".globl _stat_key_mtime\n_stat_key_mtime:\n    .ascii \"mtime\"\n");
    out.push_str(".globl _stat_key_ctime\n_stat_key_ctime:\n    .ascii \"ctime\"\n");
    out.push_str(".globl _stat_key_blksize\n_stat_key_blksize:\n    .ascii \"blksize\"\n");
    out.push_str(".globl _stat_key_blocks\n_stat_key_blocks:\n    .ascii \"blocks\"\n");
    out.push_str(".globl _dirname_dot\n_dirname_dot:\n    .ascii \".\"\n");
    out.push_str(".globl _dirname_slash\n_dirname_slash:\n    .ascii \"/\"\n");
    out.push_str(&format!(
        ".globl _dirname_levels_msg\n_dirname_levels_msg:\n    .ascii {:?}\n",
        DIRNAME_LEVELS_MSG
    ));
    out.push_str(".globl _pathinfo_key_dirname\n_pathinfo_key_dirname:\n    .ascii \"dirname\"\n");
    out.push_str(".globl _pathinfo_key_basename\n_pathinfo_key_basename:\n    .ascii \"basename\"\n");
    out.push_str(".globl _pathinfo_key_extension\n_pathinfo_key_extension:\n    .ascii \"extension\"\n");
    out.push_str(".globl _pathinfo_key_filename\n_pathinfo_key_filename:\n    .ascii \"filename\"\n");
    out.push_str(".p2align 3\n");
    out.push_str(".globl _tmpfile_template\n_tmpfile_template:\n    .ascii \"/tmp/elephc-XXXXXX\\0\"\n    .byte 0,0,0,0,0\n");
    out.push_str(".globl _locale_utf8_name\n_locale_utf8_name:\n    .asciz \"C.UTF-8\"\n");
    out.push_str(".globl _locale_env_name\n_locale_env_name:\n    .asciz \"\"\n");
    out.push_str(".globl _pcre_space\n_pcre_space:\n    .ascii \"[[:space:]]\"\n");
    out.push_str(".globl _pcre_digit\n_pcre_digit:\n    .ascii \"[[:digit:]]\"\n");
    out.push_str(".globl _pcre_word\n_pcre_word:\n    .ascii \"[[:alnum:]_]\"\n");
    out.push_str(".globl _pcre_nspace\n_pcre_nspace:\n    .ascii \"[^[:space:]]\"\n");
    out.push_str(".globl _pcre_ndigit\n_pcre_ndigit:\n    .ascii \"[^[:digit:]]\"\n");
    out.push_str(".globl _pcre_nword\n_pcre_nword:\n    .ascii \"[^[:alnum:]_]\"\n");
    out.push_str(".globl _pcre_alpha\n_pcre_alpha:\n    .ascii \"[^[:digit:][:space:][:punct:]]\"\n");
    out.push_str(".globl _pcre_nalpha\n_pcre_nalpha:\n    .ascii \"[[:digit:][:space:][:punct:]]\"\n");
    out.push_str(".globl _pcre_lower\n_pcre_lower:\n    .ascii \"[[:lower:]]\"\n");
    out.push_str(".globl _pcre_nlower\n_pcre_nlower:\n    .ascii \"[^[:lower:]]\"\n");
    out.push_str(".globl _pcre_upper\n_pcre_upper:\n    .ascii \"[[:upper:]]\"\n");
    out.push_str(".globl _pcre_nupper\n_pcre_nupper:\n    .ascii \"[^[:upper:]]\"\n");
    out.push_str(".globl _pcre_punct\n_pcre_punct:\n    .ascii \"[[:punct:]]\"\n");
    out.push_str(".globl _pcre_npunct\n_pcre_npunct:\n    .ascii \"[^[:punct:]]\"\n");
    out.push_str(&system::emit_json_data());
    out.push_str(&system::emit_date_data());
    out.push_str(&system::emit_strtotime_data());
    out.push_str(&emit_php_uname_data());

    out
}

fn emit_builtin_callable_data() -> String {
    let mut out = String::new();
    let builtins = supported_builtin_function_names();
    for (idx, name) in builtins.iter().enumerate() {
        out.push_str(&format!(
            ".globl _callable_builtin_name_{0}\n_callable_builtin_name_{0}:\n    .ascii \"{1}\"\n",
            idx, name
        ));
    }
    out.push_str(".p2align 3\n");
    out.push_str(".globl _callable_invoke_name\n_callable_invoke_name:\n");
    out.push_str("    .ascii \"__invoke\"\n");
    out.push_str(".p2align 3\n");
    out.push_str(".globl _callable_builtin_count\n_callable_builtin_count:\n");
    out.push_str(&format!("    .quad {}\n", builtins.len()));
    out.push_str(".globl _callable_builtin_table\n_callable_builtin_table:\n");
    for (idx, name) in builtins.iter().enumerate() {
        out.push_str(&format!("    .quad _callable_builtin_name_{}\n", idx));
        out.push_str(&format!("    .quad {}\n", name.len()));
    }
    out
}

fn emit_php_uname_data() -> String {
    format!(
        ".globl _php_uname_mode_len_msg\n_php_uname_mode_len_msg:\n    .ascii {:?}\n\
         .globl _php_uname_mode_value_msg\n_php_uname_mode_value_msg:\n    .ascii {:?}\n",
        PHP_UNAME_MODE_LEN_MSG, PHP_UNAME_MODE_VALUE_MSG
    )
}

/// Emit the mutable globals backing `spl_autoload_extensions` runtime
/// read/write. Initialised to point at the default ".inc,.php" string so
/// PHP programs see PHP's documented default before any explicit set.
fn emit_spl_autoload_extensions_data() -> String {
    let default = ".inc,.php";
    let mut out = String::new();
    out.push_str(".globl _spl_autoload_exts_default\n");
    out.push_str("_spl_autoload_exts_default:\n");
    out.push_str(&format!("    .ascii \"{}\"\n", default));
    out.push_str(".p2align 3\n");
    out.push_str(".globl _spl_autoload_exts_ptr\n");
    out.push_str("_spl_autoload_exts_ptr:\n");
    out.push_str("    .quad _spl_autoload_exts_default\n");
    out.push_str(".globl _spl_autoload_exts_len\n");
    out.push_str("_spl_autoload_exts_len:\n");
    out.push_str(&format!("    .quad {}\n", default.len()));
    out
}
