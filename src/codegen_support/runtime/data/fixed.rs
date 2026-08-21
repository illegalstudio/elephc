//! Purpose:
//! Builds the cacheable fixed runtime data section as assembly text.
//! This owns heap globals, shared scratch buffers, fatal messages, lookup tables, and fixed runtime state.
//!
//! Called from:
//! - `crate::codegen_support::runtime::data::emit_runtime_data_fixed()`.
//!
//! Key details:
//! - Fixed symbols are cached across compilations, so only target-independent runtime data belongs here.

use super::{
    ALLOC_OVERFLOW_MSG, ARRAY_ALLOC_SIZE_MSG, BUFFER_ALLOC_SIZE_MSG, RANGE_SIZE_MSG,
    DIRNAME_LEVELS_MSG, HASH_COPY_FINALIZED_CTX_MSG, HASH_FINAL_FINALIZED_CTX_MSG,
    HASH_HMAC_UNKNOWN_ALGO_MSG, HASH_INIT_UNKNOWN_ALGO_MSG,
    HASH_UNKNOWN_ALGO_MSG, HASH_UPDATE_FINALIZED_CTX_MSG, MB_STRLEN_UNKNOWN_ENCODING_MSG,
    OB_CLOSURE_INVOKE_NAME, OB_DEFAULT_HANDLER_NAME, OB_FATAL_IN_HANDLER, OB_NTC_CREATE_FAIL,
    OB_NTC_G_CLEAN, OB_NTC_G_END_CLEAN, OB_NTC_G_END_FLUSH, OB_NTC_G_FLUSH, OB_NTC_G_GET_CLEAN,
    OB_NTC_G_GET_FLUSH, OB_NTC_NO_CLEAN, OB_NTC_NO_END_CLEAN, OB_NTC_NO_END_FLUSH,
    OB_NTC_NO_FLUSH, OB_NTC_NO_GET_FLUSH, OBJECT_NOT_ARRAY_PREFIX, OBJECT_NOT_ARRAY_SUFFIX,
    OB_WARN_BAD_CALLBACK_GENERIC,
    OB_WARN_BAD_CALLBACK_PREFIX, OB_WARN_BAD_CALLBACK_SUFFIX,
    PHP_UNAME_MODE_LEN_MSG, PHP_UNAME_MODE_VALUE_MSG, SPRINTF_ARGCOUNT_MSG,
    SPRINTF_OVERFLOW_MSG, SPRINTF_UNKNOWN_SPEC_MSG, SPRINTF_WIDTH_MSG, STACK_OVERFLOW_MSG,
    GAI_MSG_MIDDLE, GAI_MSG_PREFIX, SOCKET_GAI_MSG_CAPACITY,
    DIAG_NEWLINE, DISK_FREE_SPACE_WARNING, DISK_TOTAL_SPACE_WARNING,
    FGC_FILTER_FAIL_TAIL, PF_WARN_CREATE_END, PF_WARN_CREATE_MID,
    PF_WARN_HEAD, PF_WARN_LOCATE_END, PF_WARN_LOCATE_MID,
    PF_WARN_OPEN_MID, SCANDIR_ERRNO_WARNING_HEAD, SCANDIR_ERRNO_WARNING_MIDDLE,
    SCANDIR_OPEN_WARNING_HEAD, SCANDIR_OPEN_WARNING_MIDDLE, DYNAMIC_PROP_DEPRECATED_HEAD,
    DYNAMIC_PROP_DEPRECATED_TAIL, FILTER_PARAM_CREATE_APPEND_HEAD, FILTER_PARAM_CREATE_PREPEND_HEAD,
    FILTER_PARAM_CREATE_TAIL, FILTER_PARAM_INVALID_APPEND_HEAD, FILTER_PARAM_INVALID_PREPEND_HEAD,
    FILTER_PARAM_INVALID_TAIL, SELECT_CAST_UNREPRESENTABLE, SELECT_CAST_UNREPRESENTABLE_MEMORY,
    SOCKET_FAILED_CLIENT_PREFIX, SOCKET_FAILED_FSOCKOPEN_PREFIX, SOCKET_FAILED_REASON_CLOSE,
    SOCKET_FAILED_REASON_OPEN, SOCKET_FAILED_REASON_UNKNOWN, SOCKET_FAILED_SERVER_PREFIX,
    UNKNOWN_WRAPPER_HEAD, UNKNOWN_WRAPPER_MIDDLE, UNKNOWN_WRAPPER_TAIL,
    SOCKET_FAILED_UNABLE, SWR_NEVER_CHANGED, SWR_NEVER_EXISTED,
    SWR_NTC_PREFIX, SWR_WRN_PREFIX, STR_REPEAT_TIMES_MSG,
    STAT_FAILED_TAIL, LSTAT_FAILED_TAIL, FILETYPE_UNKNOWN_HEAD,
    FILETYPE_UNKNOWN_TAIL, WRAPPER_MISSING_HOOK_HEAD_CHGRP, WRAPPER_MISSING_HOOK_HEAD_CHMOD,
    WRAPPER_MISSING_HOOK_HEAD_CHOWN, WRAPPER_MISSING_HOOK_HEAD_FILESIZE, WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS,
    WRAPPER_MISSING_HOOK_HEAD_IS_FILE, WRAPPER_MISSING_HOOK_HEAD_IS_DIR, WRAPPER_MISSING_HOOK_HEAD_IS_LINK,
    WRAPPER_MISSING_HOOK_HEAD_IS_READABLE, WRAPPER_MISSING_HOOK_HEAD_IS_WRITABLE, WRAPPER_MISSING_HOOK_HEAD_IS_WRITEABLE,
    WRAPPER_MISSING_HOOK_HEAD_IS_EXECUTABLE, WRAPPER_MISSING_HOOK_HEAD_FILEMTIME, WRAPPER_MISSING_HOOK_HEAD_FILEATIME,
    WRAPPER_MISSING_HOOK_HEAD_FILECTIME, WRAPPER_MISSING_HOOK_HEAD_FILETYPE, WRAPPER_MISSING_HOOK_HEAD_FILEPERMS,
    WRAPPER_MISSING_HOOK_HEAD_FILEOWNER, WRAPPER_MISSING_HOOK_HEAD_FILEGROUP, WRAPPER_MISSING_HOOK_HEAD_FILEINODE,
    WRAPPER_MISSING_HOOK_HEAD_STAT, WRAPPER_MISSING_HOOK_HEAD_LSTAT, WRAPPER_MISSING_HOOK_TAIL_URL_STAT,
    WRAPPER_MISSING_HOOK_HEAD_FEOF, WRAPPER_MISSING_HOOK_HEAD_FLOCK, WRAPPER_MISSING_HOOK_HEAD_FSTAT,
    WRAPPER_MISSING_HOOK_HEAD_FWRITE, WRAPPER_MISSING_HOOK_HEAD_MKDIR, WRAPPER_MISSING_HOOK_HEAD_RENAME,
    WRAPPER_MISSING_HOOK_HEAD_RMDIR, WRAPPER_MISSING_HOOK_HEAD_TOUCH, WRAPPER_MISSING_HOOK_HEAD_UNLINK,
    WRAPPER_MISSING_HOOK_TAIL_EOF, WRAPPER_MISSING_HOOK_TAIL_LOCK, WRAPPER_MISSING_HOOK_TAIL_METADATA,
    WRAPPER_MISSING_HOOK_TAIL_MKDIR, WRAPPER_MISSING_HOOK_TAIL_RENAME, WRAPPER_MISSING_HOOK_TAIL_RMDIR,
    WRAPPER_MISSING_HOOK_TAIL_STAT, WRAPPER_MISSING_HOOK_TAIL_UNLINK, WRAPPER_MISSING_HOOK_HEAD_SELECT,
    WRAPPER_MISSING_HOOK_TAIL_CAST, WRAPPER_MISSING_HOOK_TAIL_WRITE, UNSER_ALLOWED_CLASSES_ENTRY_PREFIX,
    UNSER_ALLOWED_CLASSES_POLICY_PREFIX, UNSER_OBJECT_STRING_ERROR_PREFIX, UNSER_OBJECT_STRING_ERROR_SUFFIX,
    UNSER_OPTIONS_TYPE_PREFIX, UNSER_TYPE_GIVEN_SUFFIX,
};
use super::super::system;
use super::RT_DIAG_BUF_BYTES;
use crate::codegen_support::data_section::comm_directive;
use crate::codegen_support::runtime::strings::{
    B64_DECODE_INVALID, B64_DECODE_SKIP, B64_DECODE_WHITESPACE,
};
use crate::codegen_support::platform::Target;
use crate::types::checker::builtins::{
    all_supported_builtin_function_names, supported_builtin_function_names_for_profile,
};

/// Emit the fixed runtime `.data` section as assembly text.
/// Cached across compilations because it contains only target-independent
/// runtime data: heap globals, concat buffers, exception/fiber state,
/// JSON/SPL error messages, base64 tables, PCRE regex patterns, and
/// lookup tables for builtins, file types, and `pathinfo` keys.
///
/// `heap_size` is the maximum heap bytes requested by the user program;
/// it is baked into `_heap_max` to enforce the heap limit at runtime.
///
/// `target` is needed only for the one symbol the `--web` bridge references
/// (`elephc_web_capture`): it must carry the platform's C-ABI mangling so the
/// runtime's `.comm`, the runtime's load, and the Rust bridge's `extern "C"`
/// all name the same symbol (`_elephc_web_capture` on macOS, `elephc_web_capture`
/// on Linux). The cache key already includes the target, so this stays cache-safe.
pub(crate) fn emit_runtime_data_fixed(heap_size: usize, target: Target) -> String {
    let mut out = String::new();
    out.push_str(".data\n");
    out.push_str(&comm_directive("_concat_buf", 65536, target));
    out.push_str(&comm_directive("_concat_off", 8, target));
    out.push_str(&comm_directive("_unser_depth", 8, target));
    out.push_str(".globl _unser_depth_msg\n_unser_depth_msg:\n    .ascii \"Fatal error: maximum unserialize depth exceeded\\n\"\n");
    out.push_str(&comm_directive("_unser_allowed_mode", 8, target));
    out.push_str(&comm_directive("_unser_allowed_list", 8, target));
    out.push_str(&comm_directive("_unser_allowed_list_mixed", 8, target));
    out.push_str(&comm_directive("_unser_active", 8, target));
    out.push_str(&comm_directive("_unser_context", 8, target));
    out.push_str(".globl _unser_allowed_classes_key\n_unser_allowed_classes_key:\n    .ascii \"allowed_classes\"\n");
    out.push_str(&format!(
        ".globl _unser_options_type_prefix\n_unser_options_type_prefix:\n    .ascii {UNSER_OPTIONS_TYPE_PREFIX:?}\n"
    ));
    out.push_str(&format!(
        ".globl _unser_allowed_classes_policy_prefix\n_unser_allowed_classes_policy_prefix:\n    .ascii {UNSER_ALLOWED_CLASSES_POLICY_PREFIX:?}\n"
    ));
    out.push_str(&format!(
        ".globl _unser_allowed_classes_entry_prefix\n_unser_allowed_classes_entry_prefix:\n    .ascii {UNSER_ALLOWED_CLASSES_ENTRY_PREFIX:?}\n"
    ));
    out.push_str(&format!(
        ".globl _object_not_array_prefix\n_object_not_array_prefix:\n    .ascii {OBJECT_NOT_ARRAY_PREFIX:?}\n"
    ));
    out.push_str(&format!(
        ".globl _object_not_array_suffix\n_object_not_array_suffix:\n    .ascii {OBJECT_NOT_ARRAY_SUFFIX:?}\n"
    ));
    out.push_str(&format!(
        ".globl _unser_object_string_error_prefix\n_unser_object_string_error_prefix:\n    .ascii {UNSER_OBJECT_STRING_ERROR_PREFIX:?}\n"
    ));
    out.push_str(&format!(
        ".globl _unser_object_string_error_suffix\n_unser_object_string_error_suffix:\n    .ascii {UNSER_OBJECT_STRING_ERROR_SUFFIX:?}\n"
    ));
    out.push_str(&format!(
        ".globl _unser_type_given_suffix\n_unser_type_given_suffix:\n    .ascii {UNSER_TYPE_GIVEN_SUFFIX:?}\n"
    ));
    for (label, name) in [
        ("_unser_type_int", "int"),
        ("_unser_type_string", "string"),
        ("_unser_type_float", "float"),
        ("_unser_type_bool", "bool"),
        ("_unser_type_array", "array"),
        ("_unser_type_object", "object"),
        ("_unser_type_null", "null"),
        ("_unser_type_resource", "resource"),
        ("_unser_type_unknown", "unknown"),
    ] {
        out.push_str(&format!(
            ".globl {label}\n{label}:\n    .ascii {name:?}\n"
        ));
    }
    out.push_str(".globl _incomplete_class_name\n_incomplete_class_name:\n    .ascii \"__PHP_Incomplete_Class\"\n");
    out.push_str(".globl _incomplete_class_property_name\n_incomplete_class_property_name:\n    .ascii \"__PHP_Incomplete_Class_Name\"\n");
    // print_r($value, true) return-mode capture state. _print_r_mode is a flag
    // (0 = write to stdout, 1 = append to _print_r_buf) consulted by
    // __rt_stdout_write and __rt_pr_write; _print_r_off tracks the accumulated
    // byte count; _print_r_buf is the 64 KiB accumulation buffer finalized by
    // __rt_pr_finish into an owned heap string. Only non-zero during an active
    // print_r return-mode rendering, so non-print_r output is unaffected.
    out.push_str(&comm_directive("_print_r_mode", 8, target));
    out.push_str(&comm_directive("_print_r_off", 8, target));
    out.push_str(&comm_directive("_print_r_buf", 65536, target));
    // Output-buffering (ob_*) stack state. _ob_level is the active nesting depth
    // (0 = no buffering) consulted by __rt_stdout_write and __rt_pr_write before
    // the terminal write syscall; _ob_ptrs/_ob_lens/_ob_caps are 64-slot parallel
    // arrays (heap buffer base pointer, used bytes, capacity) indexed by level-1.
    // Buffers are heap-allocated by __rt_ob_start, grown by __rt_ob_append, and
    // written to the terminal sink by __rt_ob_flush_all at process exit.
    out.push_str(&comm_directive("_ob_level", 8, target));
    out.push_str(&comm_directive("_ob_ptrs", 512, target));
    out.push_str(&comm_directive("_ob_lens", 512, target));
    out.push_str(&comm_directive("_ob_caps", 512, target));
    // Per-level output-buffer metadata (parallel to _ob_ptrs, indexed by level-1):
    // the user-handler invocation stub + env word (stub 0 = default handler; env
    // is a retained callable-descriptor pointer for AOT handlers or a magician
    // registry id for eval handlers), the persisted handler display name
    // (ptr/len), the auto-flush chunk size, the ob_start() flags word, and the
    // started flag (set at the first handler invocation; feeds PHP started bits).
    out.push_str(&comm_directive("_ob_handler_stubs", 512, target));
    out.push_str(&comm_directive("_ob_handler_envs", 512, target));
    out.push_str(&comm_directive("_ob_name_ptrs", 512, target));
    out.push_str(&comm_directive("_ob_name_lens", 512, target));
    out.push_str(&comm_directive("_ob_chunk_sizes", 512, target));
    out.push_str(&comm_directive("_ob_flags", 512, target));
    out.push_str(&comm_directive("_ob_started", 512, target));
    // _ob_in_handler: non-zero while a user output handler runs. Output produced
    // inside a handler is discarded (PHP behavior) via the __rt_stdout_write and
    // __rt_pr_write branches, and ob_start() inside a handler is a fatal error.
    out.push_str(&comm_directive("_ob_in_handler", 8, target));
    // _ob_flushing: re-entry guard for the process-exit drain. A user handler
    // running during __rt_ob_flush_all may call exit() again; the guard makes
    // the nested drain a no-op instead of an infinite loop.
    out.push_str(&comm_directive("_ob_flushing", 8, target));
    // _elephc_eval_ob_handler_fn: installed Rust callback (magician) that runs
    // an eval-registered ob_start() handler: fn(id, buf, len, phase) -> Mixed
    // result cell pointer (0 = pass-through). Called via __rt_ob_eval_trampoline.
    out.push_str(&comm_directive("_elephc_eval_ob_handler_fn", 8, target));
    // "Closure::__invoke": PHP display name for closure / first-class-callable
    // output handlers in ob_get_status()/ob_list_handlers().
    out.push_str(&format!(
        ".globl _ob_closure_invoke_name\n_ob_closure_invoke_name:\n    .ascii {OB_CLOSURE_INVOKE_NAME:?}\n"
    ));
    // ob_implicit_flush() stored flag. Semantically inert in elephc: terminal
    // writes are unbuffered syscalls, so implicit flushing is always on.
    out.push_str(&comm_directive("_ob_implicit_flush", 8, target));
    // ob_get_status()/ob_list_handlers() string constants: PHP's default handler
    // name and the status-array key strings read by __rt_ob_get_status.
    out.push_str(&format!(
        ".globl _ob_handler_name\n_ob_handler_name:\n    .ascii {OB_DEFAULT_HANDLER_NAME:?}\n"
    ));
    for (sym, key) in [
        ("_ob_k_name", "name"),
        ("_ob_k_type", "type"),
        ("_ob_k_flags", "flags"),
        ("_ob_k_level", "level"),
        ("_ob_k_chunk_size", "chunk_size"),
        ("_ob_k_buffer_size", "buffer_size"),
        ("_ob_k_buffer_used", "buffer_used"),
    ] {
        out.push_str(&format!(".globl {sym}\n{sym}:\n    .ascii \"{key}\"\n"));
    }
    // ob_* PHP-parity diagnostics (texts shared with the ob_* runtime emitters
    // via runtime::data consts so byte lengths stay single-sourced). The
    // no-buffer notices are complete lines; the flags-gated notices are
    // prefixes completed at runtime with the handler display name and
    // " (LEVEL)\n" via __rt_ob_notice_named. All are ordinary output routed
    // through __rt_stdout_write, so active parent buffers capture them exactly
    // like PHP with display_errors enabled.
    for (label, message) in [
        ("_swr_ntc_prefix", SWR_NTC_PREFIX),
        ("_swr_wrn_prefix", SWR_WRN_PREFIX),
        ("_swr_never_changed", SWR_NEVER_CHANGED),
        ("_swr_never_existed", SWR_NEVER_EXISTED),
        ("_sock_fail_client", SOCKET_FAILED_CLIENT_PREFIX),
        ("_sock_fail_server", SOCKET_FAILED_SERVER_PREFIX),
        ("_sock_fail_fsockopen", SOCKET_FAILED_FSOCKOPEN_PREFIX),
        ("_sock_fail_unable", SOCKET_FAILED_UNABLE),
        ("_sock_fail_newline", "\n"),
        ("_sock_fail_open", SOCKET_FAILED_REASON_OPEN),
        ("_sock_fail_unknown", SOCKET_FAILED_REASON_UNKNOWN),
        ("_sock_fail_close", SOCKET_FAILED_REASON_CLOSE),
        ("_sock_fail_colon", ":"),
        ("_diag_newline", DIAG_NEWLINE),
        ("_dyn_prop_dep_head", DYNAMIC_PROP_DEPRECATED_HEAD),
        ("_dyn_prop_dep_tail", DYNAMIC_PROP_DEPRECATED_TAIL),
        ("_disk_free_space_warn", DISK_FREE_SPACE_WARNING),
        ("_disk_total_space_warn", DISK_TOTAL_SPACE_WARNING),
        ("_fgc_mode_r", "r"),
        ("_fpc_mode_w", "w"),
        ("_fpc_mode_a", "a"),
        ("_fgc_filter_fail_tail", FGC_FILTER_FAIL_TAIL),
        ("_pf_w_head", PF_WARN_HEAD),
        ("_pf_w_locate_mid", PF_WARN_LOCATE_MID),
        ("_pf_w_locate_end", PF_WARN_LOCATE_END),
        ("_pf_w_create_mid", PF_WARN_CREATE_MID),
        ("_pf_w_create_end", PF_WARN_CREATE_END),
        ("_pf_w_open_mid", PF_WARN_OPEN_MID),
        ("_uwmh_head_fwrite", WRAPPER_MISSING_HOOK_HEAD_FWRITE),
        ("_uwmh_head_feof", WRAPPER_MISSING_HOOK_HEAD_FEOF),
        ("_uwmh_head_fstat", WRAPPER_MISSING_HOOK_HEAD_FSTAT),
        ("_uwmh_head_flock", WRAPPER_MISSING_HOOK_HEAD_FLOCK),
        ("_uwmh_tail_write", WRAPPER_MISSING_HOOK_TAIL_WRITE),
        ("_uwmh_head_select", WRAPPER_MISSING_HOOK_HEAD_SELECT),
        ("_uwmh_tail_cast", WRAPPER_MISSING_HOOK_TAIL_CAST),
        ("_rt_diag_nl", "\n"),
        ("_rt_diag_in", " in "),
        ("_select_cast_unrepresentable", SELECT_CAST_UNREPRESENTABLE),
        ("_select_cast_unrepresentable_memory", SELECT_CAST_UNREPRESENTABLE_MEMORY),
        ("_uwmh_tail_eof", WRAPPER_MISSING_HOOK_TAIL_EOF),
        ("_uwmh_tail_stat", WRAPPER_MISSING_HOOK_TAIL_STAT),
        ("_uwmh_tail_lock", WRAPPER_MISSING_HOOK_TAIL_LOCK),
        ("_uwmh_head_unlink", WRAPPER_MISSING_HOOK_HEAD_UNLINK),
        ("_uwmh_head_rename", WRAPPER_MISSING_HOOK_HEAD_RENAME),
        ("_uwmh_head_mkdir", WRAPPER_MISSING_HOOK_HEAD_MKDIR),
        ("_uwmh_head_rmdir", WRAPPER_MISSING_HOOK_HEAD_RMDIR),
        ("_uwmh_head_chmod", WRAPPER_MISSING_HOOK_HEAD_CHMOD),
        ("_uwmh_head_touch", WRAPPER_MISSING_HOOK_HEAD_TOUCH),
        ("_uwmh_head_chown", WRAPPER_MISSING_HOOK_HEAD_CHOWN),
        ("_uwmh_head_chgrp", WRAPPER_MISSING_HOOK_HEAD_CHGRP),
        ("_uwmh_tail_unlink", WRAPPER_MISSING_HOOK_TAIL_UNLINK),
        ("_uwmh_tail_rename", WRAPPER_MISSING_HOOK_TAIL_RENAME),
        ("_uwmh_tail_mkdir", WRAPPER_MISSING_HOOK_TAIL_MKDIR),
        ("_uwmh_tail_rmdir", WRAPPER_MISSING_HOOK_TAIL_RMDIR),
        ("_uwmh_tail_metadata", WRAPPER_MISSING_HOOK_TAIL_METADATA),
        ("_uwmh_head_file_exists", WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS),
        ("_uwmh_head_filesize", WRAPPER_MISSING_HOOK_HEAD_FILESIZE),
        ("_uwmh_head_is_file", WRAPPER_MISSING_HOOK_HEAD_IS_FILE),
        ("_uwmh_head_is_dir", WRAPPER_MISSING_HOOK_HEAD_IS_DIR),
        ("_uwmh_head_is_link", WRAPPER_MISSING_HOOK_HEAD_IS_LINK),
        ("_uwmh_head_is_readable", WRAPPER_MISSING_HOOK_HEAD_IS_READABLE),
        ("_uwmh_head_is_writable", WRAPPER_MISSING_HOOK_HEAD_IS_WRITABLE),
        ("_uwmh_head_is_writeable", WRAPPER_MISSING_HOOK_HEAD_IS_WRITEABLE),
        ("_uwmh_head_is_executable", WRAPPER_MISSING_HOOK_HEAD_IS_EXECUTABLE),
        ("_uwmh_head_filemtime", WRAPPER_MISSING_HOOK_HEAD_FILEMTIME),
        ("_uwmh_head_fileatime", WRAPPER_MISSING_HOOK_HEAD_FILEATIME),
        ("_uwmh_head_filectime", WRAPPER_MISSING_HOOK_HEAD_FILECTIME),
        ("_uwmh_head_filetype", WRAPPER_MISSING_HOOK_HEAD_FILETYPE),
        ("_uwmh_head_fileperms", WRAPPER_MISSING_HOOK_HEAD_FILEPERMS),
        ("_uwmh_head_fileowner", WRAPPER_MISSING_HOOK_HEAD_FILEOWNER),
        ("_uwmh_head_filegroup", WRAPPER_MISSING_HOOK_HEAD_FILEGROUP),
        ("_uwmh_head_fileinode", WRAPPER_MISSING_HOOK_HEAD_FILEINODE),
        ("_uwmh_head_stat", WRAPPER_MISSING_HOOK_HEAD_STAT),
        ("_uwmh_head_lstat", WRAPPER_MISSING_HOOK_HEAD_LSTAT),
        ("_uwmh_tail_url_stat", WRAPPER_MISSING_HOOK_TAIL_URL_STAT),
        ("_stat_failed_tail", STAT_FAILED_TAIL),
        ("_lstat_failed_tail", LSTAT_FAILED_TAIL),
        ("_filetype_unknown_head", FILETYPE_UNKNOWN_HEAD),
        ("_filetype_unknown_tail", FILETYPE_UNKNOWN_TAIL),
        ("_scandir_open_warn_head", SCANDIR_OPEN_WARNING_HEAD),
        ("_scandir_open_warn_mid", SCANDIR_OPEN_WARNING_MIDDLE),
        ("_scandir_errno_warn_head", SCANDIR_ERRNO_WARNING_HEAD),
        ("_scandir_errno_warn_mid", SCANDIR_ERRNO_WARNING_MIDDLE),
        ("_unknown_wrapper_head", UNKNOWN_WRAPPER_HEAD),
        ("_unknown_wrapper_mid", UNKNOWN_WRAPPER_MIDDLE),
        ("_unknown_wrapper_tail", UNKNOWN_WRAPPER_TAIL),
        ("_ob_ntc_no_end_flush", OB_NTC_NO_END_FLUSH),
        ("_ob_ntc_no_get_flush", OB_NTC_NO_GET_FLUSH),
        ("_ob_ntc_no_end_clean", OB_NTC_NO_END_CLEAN),
        ("_ob_ntc_no_flush", OB_NTC_NO_FLUSH),
        ("_ob_ntc_no_clean", OB_NTC_NO_CLEAN),
        ("_ob_ntc_g_clean", OB_NTC_G_CLEAN),
        ("_ob_ntc_g_flush", OB_NTC_G_FLUSH),
        ("_ob_ntc_g_end_clean", OB_NTC_G_END_CLEAN),
        ("_ob_ntc_g_get_clean", OB_NTC_G_GET_CLEAN),
        ("_ob_ntc_g_end_flush", OB_NTC_G_END_FLUSH),
        ("_ob_ntc_g_get_flush", OB_NTC_G_GET_FLUSH),
        ("_ob_ntc_g_open", " ("),
        ("_ob_ntc_g_close", ")\n"),
        ("_ob_warn_bad_callback_prefix", OB_WARN_BAD_CALLBACK_PREFIX),
        ("_ob_warn_bad_callback_suffix", OB_WARN_BAD_CALLBACK_SUFFIX),
        ("_ob_warn_bad_callback_generic", OB_WARN_BAD_CALLBACK_GENERIC),
        ("_ob_ntc_create_fail", OB_NTC_CREATE_FAIL),
        ("_ob_fatal_in_handler", OB_FATAL_IN_HANDLER),
    ] {
        out.push_str(&format!(".globl {label}\n{label}:\n    .ascii {message:?}\n"));
    }
    // The count() TypeError texts, published under the labels `__rt_count_type_message` indexes.
    for (label, message) in crate::codegen_support::runtime::arrays::count_type_error_symbols() {
        out.push_str(&format!(".globl {label}\n{label}:\n    .ascii {message:?}\n"));
    }
    // serialize()/unserialize() reference tracking (PHP r:/R: back-references).
    // serialize: a global value counter (every serialized value consumes the next
    // index, keys excluded) plus a pointer->index map of already-serialized objects
    // (parallel arrays, linear scan) so a repeated object emits r:<index>. unserialize:
    // a registry of created value boxes indexed by the same pre-order counter so r:<N>
    // resolves to the existing value. Reentrant calls snapshot the used prefix plus their
    // policy/depth fields through _unser_context. Capacity bounds the per-call object/value
    // count; overflow degrades gracefully (serialize stops deduping, unserialize fails the ref).
    out.push_str(&comm_directive("_ser_value_counter", 8, target));
    out.push_str(&comm_directive("_ser_obj_count", 8, target));
    out.push_str(&comm_directive("_ser_obj_ptrs", 524288, target));
    out.push_str(&comm_directive("_ser_obj_idxs", 524288, target));
    out.push_str(&comm_directive("_unser_count", 8, target));
    out.push_str(&comm_directive("_unser_values", 524288, target));
    out.push_str(&comm_directive("_strtotime_clock", 8, target));
    // Default-timezone state: the "TZ=<id>" env buffer (kept alive for putenv), the stored
    // identifier length (0 = none set → date_default_timezone_get returns "UTC"), and the
    // "UTC" literal returned in that default case.
    out.push_str(&comm_directive("_php_tz_env", 264, target));
    out.push_str(&comm_directive("_php_default_tz_len", 8, target));
    out.push_str(&comm_directive("_php_tz_save", 264, target));
    out.push_str(".globl _php_tz_utc\n");
    out.push_str("_php_tz_utc:\n");
    out.push_str("    .ascii \"UTC\"\n");
    // getdate() associative-array key strings (read by __rt_getdate).
    for (sym, key) in [
        ("_gd_k_seconds", "seconds"),
        ("_gd_k_minutes", "minutes"),
        ("_gd_k_hours", "hours"),
        ("_gd_k_mday", "mday"),
        ("_gd_k_wday", "wday"),
        ("_gd_k_mon", "mon"),
        ("_gd_k_year", "year"),
        ("_gd_k_yday", "yday"),
        ("_gd_k_weekday", "weekday"),
        ("_gd_k_month", "month"),
    ] {
        out.push_str(&format!(".globl {sym}\n{sym}:\n    .ascii \"{key}\"\n"));
    }
    // localtime() associative-array key strings (read by __rt_localtime).
    for key in [
        "tm_sec", "tm_min", "tm_hour", "tm_mday", "tm_mon", "tm_year", "tm_wday", "tm_yday",
        "tm_isdst",
    ] {
        out.push_str(&format!(".globl _lt_k_{key}\n_lt_k_{key}:\n    .ascii \"{key}\"\n"));
    }
    out.push_str(&comm_directive("_global_argc", 8, target));
    out.push_str(&comm_directive("_global_argv", 8, target));
    out.push_str(&comm_directive("_exc_handler_top", 8, target));
    out.push_str(&comm_directive("_exc_call_frame_top", 8, target));
    out.push_str(&comm_directive("_exc_value", 8, target));
    out.push_str(&comm_directive("_fiber_current", 8, target));
    out.push_str(&comm_directive("_fiber_main_saved_sp", 8, target));
    out.push_str(&comm_directive("_fiber_main_saved_exc", 8, target));
    out.push_str(&comm_directive("_fiber_main_saved_call_frame", 8, target));
    // Call-stack overflow guard state. _stack_limit is the low-water stack address of the
    // execution context that is running right now: every compiled function prologue does an
    // unsigned compare of the stack pointer against it and branches to __rt_stack_overflow
    // when it is below. Zero (the .comm default) disables the guard, so a program that never
    // runs __rt_stack_limit_init keeps the pre-guard behavior. _stack_limit_main remembers
    // the OS-thread floor so __rt_fiber_switch can restore it when control leaves a fiber
    // stack; while a fiber runs, _stack_limit holds that fiber's own floor instead.
    out.push_str(&comm_directive("_stack_limit", 8, target));
    out.push_str(&comm_directive("_stack_limit_main", 8, target));
    out.push_str(&comm_directive("_elephc_eval_dynamic_object_destruct_fn", 8, target));
    out.push_str(&comm_directive("_rt_diag_suppression", 8, target));
    // The diagnostic LINE BUFFER and the location it is stamped with.
    //
    // php prints a warning as one line — a blank line, the message, ` in FILE on line N` — and
    // routes it through the output buffer, so `ob_start()` captures it like any echo. elephc
    // composes a message in several `__rt_diag_warning` calls (head, name, tail), so the pieces
    // are accumulated here and written together once the piece carrying the newline arrives.
    // Without that the location could only be appended per PIECE, three times per line.
    //
    // `_rt_diag_loc_ptr`/`_rt_diag_loc_len` hold the pre-rendered ` in FILE on line N\n` for the
    // site about to warn. It is rendered at COMPILE time — both halves are constants there — so
    // no integer formatting is needed at run time. A zero length means no site published one and
    // the line ends with a bare newline, which keeps a partially-covered build well-formed.
    out.push_str(&comm_directive("_rt_diag_buf", RT_DIAG_BUF_BYTES, target));
    out.push_str(&comm_directive("_rt_diag_buf_len", 8, target));
    out.push_str(&comm_directive("_rt_diag_loc_ptr", 8, target));
    out.push_str(&comm_directive("_rt_diag_loc_len", 8, target));
    // elephc_web_capture: per-request output-capture mode flag read by
    // __rt_stdout_write. Zero (the default) routes echo output to the plain
    // write(1, …) syscall; non-zero (set only by the --web bridge) routes it to
    // elephc_web_write so the response body can be captured per request. Only the
    // low byte is used, but the 8-byte/align-3 house style keeps it word-aligned.
    // The symbol is mangled per-target so the bridge's `extern "C"` declaration
    // resolves to it on every platform (see emit_runtime_data_fixed docs).
    out.push_str(&comm_directive(
        &target.extern_symbol("elephc_web_capture"),
        8,
        target,
    ));
    out.push_str(&comm_directive("_heap_buf", heap_size, target));
    out.push_str(&comm_directive("_heap_off", 8, target));
    out.push_str(&comm_directive("_heap_free_list", 8, target));
    out.push_str(&comm_directive("_heap_small_bins", 32, target));
    out.push_str(&comm_directive("_heap_debug_enabled", 8, target));
    out.push_str(&comm_directive("_web_heap_guard_enabled", 8, target));
    // Generation-safe buffer descriptor registry. Public Buffer values are
    // scalar `(generation << 32) | index` handles, never heap pointers: slot
    // reuse increments generation so stale aliases cannot access a new payload.
    // Index zero is reserved as the invalid/null handle; the descriptor free
    // list is static metadata and therefore does not consume user heap space.
    out.push_str(&comm_directive(
        "_buffer_registry",
        (crate::codegen_support::runtime::buffers::BUFFER_REGISTRY_CAPACITY + 1)
            * crate::codegen_support::runtime::buffers::BUFFER_DESCRIPTOR_SIZE,
        target,
    ));
    out.push_str(&comm_directive("_buffer_registry_free", 8, target));
    out.push_str(".globl _buffer_registry_next\n_buffer_registry_next:\n    .quad 1\n");
    // PHP object-handle pool. `_obj_handle_index` is a DIRECT-MAPPED side table
    // holding one u32 handle per 16-byte granule of `_heap_buf`: two live heap
    // blocks can never share a granule because the smallest block is 16 header
    // bytes plus the allocator's 8-byte minimum payload = 24 > 16, so the mapping
    // needs no hashing, no probing and no capacity policy. It stores handles keyed
    // BY POSITION and never an object pointer, so it owns nothing, keeps nothing
    // alive and is never a GC root. `_obj_handle_free` is the LIFO stack of
    // released handles php-src reuses from; its depth can never exceed the number
    // of distinct handles, which is bounded by the peak live-object count, which is
    // bounded by `heap_size / 24`. See `runtime::objects::handles`.
    out.push_str(&comm_directive(
        "_obj_handle_index",
        crate::codegen_support::runtime::object_handle_index_slots(heap_size) * 4,
        target,
    ));
    out.push_str(&comm_directive(
        "_obj_handle_free",
        crate::codegen_support::runtime::object_handle_free_slots(heap_size) * 4,
        target,
    ));
    out.push_str(&comm_directive("_obj_handle_free_top", 8, target));
    // PHP RESOURCE ids. A SEPARATE numbering space from the object handles above —
    // php-src keeps `zend_resource.handle` and `zend_object.handle` in two unrelated
    // lists, so `resource(5)` and `object(C)#5` can and do coexist. The table maps a
    // native resource payload (a file descriptor, a DIR*, an elephc-crypto
    // HashContext handle) to the small integer PHP shows. It is an OPEN-ADDRESSED
    // hash rather than the direct-mapped granule table the object pool uses, because
    // resource payloads are not heap-block addresses: descriptors are tiny integers
    // and bridge handles come from the C allocator, so neither has a granule to index.
    // Occupancy lives in the value word (id 0 = empty slot; minted ids start at 5).
    out.push_str(&comm_directive(
        "_resource_id_keys",
        crate::codegen_support::runtime::RESOURCE_ID_TABLE_SLOTS * 8,
        target,
    ));
    out.push_str(&comm_directive(
        "_resource_id_vals",
        crate::codegen_support::runtime::RESOURCE_ID_TABLE_SLOTS * 8,
        target,
    ));
    out.push_str(&comm_directive("_gc_collecting", 8, target));
    out.push_str(&comm_directive("_gc_release_suppressed", 8, target));
    out.push_str(&comm_directive("_json_last_error", 8, target));
    out.push_str(&comm_directive("_json_active_flags", 8, target));
    out.push_str(&comm_directive("_json_active_depth", 8, target));
    out.push_str(&comm_directive("_json_indent_depth", 8, target));
    out.push_str(&comm_directive("_json_depth_limit", 8, target));
    out.push_str(&comm_directive("_json_validate_idx", 8, target));
    out.push_str(&comm_directive("_json_validate_ptr", 8, target));
    out.push_str(&comm_directive("_json_validate_len", 8, target));
    out.push_str(&comm_directive("_json_decode_assoc", 8, target));
    out.push_str(&comm_directive("_json_error_source_ptr", 8, target));
    out.push_str(&comm_directive("_json_error_location_active", 8, target));
    out.push_str(&comm_directive("_json_error_line", 8, target));
    out.push_str(&comm_directive("_json_error_column", 8, target));
    // `_obj_handle_next` is the never-used PHP object-handle cursor. PHP's first
    // object is `#1`, so the pool starts at 1 and handle 0 is reserved to mean
    // "this block never acquired a handle".
    out.push_str(".globl _obj_handle_next\n_obj_handle_next:\n    .quad 1\n");
    // `_resource_id_next` is the never-reused PHP RESOURCE id cursor. It starts at 4,
    // and that number is measured rather than chosen: under PHP 8.5.6 CLI the three
    // standard streams occupy ids 1..3 (`get_resource_id(STDIN|STDOUT|STDERR)` returns
    // 1, 2, 3) and id 4 is the REQUEST DEFAULT STREAM CONTEXT — not, as this comment
    // previously assumed, an opaque resource the SAPI consumes. That is directly
    // observable: `$d = opendir("."); var_dump(get_resource_id($d));` prints 5 while a
    // following `stream_context_get_default()` prints 4, so the context was created
    // BEFORE the stream, by the stream open itself. Two calls to
    // `stream_context_get_default()` both answer 4, so it is created once and retained.
    //
    // The cursor therefore seeds at 4 and the default context takes it, leaving 5 for
    // the first stream a script opens. Seeding at 5 instead made the context consume 5
    // and shifted every user resource by one.
    out.push_str(".globl _resource_id_next\n_resource_id_next:\n    .quad 4\n");
    // Gate 1 opaque resource registry. Handles contain only a generation and a
    // one-based slot index; no OS descriptor is PHP-visible.
    //
    // THE INITIAL SLOT ARRAY IS STATIC, NOT HEAP. Allocating it in every program's
    // prologue cost 512 bytes of runtime heap before the first PHP statement ran, so a
    // program compiled with a small `--heap-size` died at startup with
    // `Fatal error: heap memory exhausted` — 256-byte harnesses could not run at all.
    // Reserving it here costs nothing at run time, keeps `__rt_heap_alloc` untouched for
    // the program's own allocations, and removes the block that used to be reported as
    // leaked in every stream-free program. Growth still moves to the heap; the growth and
    // teardown paths recognize this base and never hand it to `__rt_heap_free`.
    out.push_str(&comm_directive("_resource_registry_static_slots", 512, target));
    out.push_str(&comm_directive("_resource_registry_ptr", 8, target));
    out.push_str(&comm_directive("_resource_registry_len", 8, target));
    out.push_str(&comm_directive("_resource_registry_cap", 8, target));
    out.push_str(&comm_directive("_resource_registry_free", 8, target));
    out.push_str(&comm_directive("_resource_registry_live", 8, target));
    out.push_str(&comm_directive("_resource_registry_epoch", 8, target));
    // Persistent 320-byte StreamState records for STDIN/STDOUT/STDERR. Their
    // registry slots use generation one and PHP constants carry the corresponding
    // opaque handles rather than exposing descriptors zero through two.
    out.push_str(&comm_directive("_resource_std_stream_states", 960, target));
    out.push_str(".globl _resource_std_stream_uri_stdin\n_resource_std_stream_uri_stdin:\n    .ascii \"php://stdin\"\n");
    out.push_str(".globl _resource_std_stream_uri_stdout\n_resource_std_stream_uri_stdout:\n    .ascii \"php://stdout\"\n");
    out.push_str(".globl _resource_std_stream_uri_stderr\n_resource_std_stream_uri_stderr:\n    .ascii \"php://stderr\"\n");
    out.push_str(".p2align 3\n.globl _resource_std_stream_uri_ptrs\n_resource_std_stream_uri_ptrs:\n    .quad _resource_std_stream_uri_stdin\n    .quad _resource_std_stream_uri_stdout\n    .quad _resource_std_stream_uri_stderr\n");
    out.push_str(".globl _resource_std_stream_uri_lens\n_resource_std_stream_uri_lens:\n    .quad 11\n    .quad 12\n    .quad 12\n");
    out.push_str(&format!(".globl _heap_max\n_heap_max:\n    .quad {}\n", heap_size));
    out.push_str(".globl _heap_err_msg\n_heap_err_msg:\n    .ascii \"Fatal error: heap memory exhausted\\n\"\n");
    out.push_str(".globl _heap_dbg_bad_refcount_msg\n_heap_dbg_bad_refcount_msg:\n    .ascii \"Fatal error: heap debug detected bad refcount\\n\"\n");
    out.push_str(".globl _heap_dbg_double_free_msg\n_heap_dbg_double_free_msg:\n    .ascii \"Fatal error: heap debug detected double free\\n\"\n");
    out.push_str(".globl _heap_dbg_free_list_msg\n_heap_dbg_free_list_msg:\n    .ascii \"Fatal error: heap debug detected free-list corruption\\n\"\n");
    out.push_str(&format!(
        ".globl _stack_err_msg\n_stack_err_msg:\n    .ascii {:?}\n",
        STACK_OVERFLOW_MSG
    ));
    out.push_str(&format!(
        ".globl _arr_cap_err_msg\n_arr_cap_err_msg:\n    .ascii {:?}\n",
        ARRAY_ALLOC_SIZE_MSG
    ));
    out.push_str(&format!(
        ".globl _range_size_err_msg\n_range_size_err_msg:\n    .ascii {:?}\n",
        RANGE_SIZE_MSG
    ));
    out.push_str(&format!(
        ".globl _buffer_alloc_size_msg\n_buffer_alloc_size_msg:\n    .ascii {:?}\n",
        BUFFER_ALLOC_SIZE_MSG
    ));
    out.push_str(".globl _buffer_bounds_msg\n_buffer_bounds_msg:\n    .ascii \"Fatal error: buffer index out of bounds\\n\"\n");
    out.push_str(".globl _buffer_uaf_msg\n_buffer_uaf_msg:\n    .ascii \"Fatal error: use of buffer after buffer_free()\\n\"\n");
    out.push_str(".globl _buffer_registry_exhausted_msg\n_buffer_registry_exhausted_msg:\n    .ascii \"Fatal error: buffer registry exhausted\\n\"\n");
    out.push_str(".globl _closure_bind_unsupported_msg\n_closure_bind_unsupported_msg:\n    .ascii \"Fatal error: Closure::bind requires a closure that captures only $this\\n\"\n");
    out.push_str(".globl _iterable_unsupported_kind_msg\n_iterable_unsupported_kind_msg:\n    .ascii \"Fatal error: foreach over iterable with unsupported kind\\n\"\n");
    out.push_str(".globl _iterable_array_str\n_iterable_array_str:\n    .ascii \"Array\"\n");
    out.push_str(".globl _match_unhandled_msg\n_match_unhandled_msg:\n    .ascii \"Fatal error: unhandled match case\\n\"\n");
    out.push_str(".globl _static_prop_private_access_msg\n_static_prop_private_access_msg:\n    .ascii \"Fatal error: Cannot access private static property\\n\"\n");
    out.push_str(".globl _ptr_null_err_msg\n_ptr_null_err_msg:\n    .ascii \"Fatal error: null pointer dereference\\n\"\n");
    out.push_str(".globl _ptr_read_string_len_err_msg\n_ptr_read_string_len_err_msg:\n    .ascii \"Fatal error: ptr_read_string() length must be non-negative\\n\"\n");
    out.push_str(&format!(
        ".globl _alloc_overflow_msg\n_alloc_overflow_msg:\n    .ascii {:?}\n",
        ALLOC_OVERFLOW_MSG
    ));
    out.push_str(&format!(
        ".globl _str_repeat_times_msg\n_str_repeat_times_msg:\n    .ascii {:?}\n",
        STR_REPEAT_TIMES_MSG
    ));
    out.push_str(&format!(
        ".globl _sprintf_width_msg\n_sprintf_width_msg:\n    .ascii {:?}\n",
        SPRINTF_WIDTH_MSG
    ));
    out.push_str(&format!(
        ".globl _sprintf_overflow_msg\n_sprintf_overflow_msg:\n    .ascii {:?}\n",
        SPRINTF_OVERFLOW_MSG
    ));
    out.push_str(&format!(
        ".globl _sprintf_argcount_msg\n_sprintf_argcount_msg:\n    .ascii {:?}\n",
        SPRINTF_ARGCOUNT_MSG
    ));
    out.push_str(&format!(
        ".globl _sprintf_unknown_spec_msg\n_sprintf_unknown_spec_msg:\n    .ascii {:?}\n",
        SPRINTF_UNKNOWN_SPEC_MSG
    ));
    out.push_str(&format!(
        ".globl _hash_unknown_algo_msg\n_hash_unknown_algo_msg:\n    .ascii {:?}\n",
        HASH_UNKNOWN_ALGO_MSG
    ));
    out.push_str(&format!(
        ".globl _hash_hmac_unknown_algo_msg\n_hash_hmac_unknown_algo_msg:\n    .ascii {:?}\n",
        HASH_HMAC_UNKNOWN_ALGO_MSG
    ));
    out.push_str(&format!(
        ".globl _hash_init_unknown_algo_msg\n_hash_init_unknown_algo_msg:\n    .ascii {:?}\n",
        HASH_INIT_UNKNOWN_ALGO_MSG
    ));
    out.push_str(&format!(
        ".globl _hash_update_finalized_ctx_msg\n_hash_update_finalized_ctx_msg:\n    .ascii {:?}\n",
        HASH_UPDATE_FINALIZED_CTX_MSG
    ));
    out.push_str(&format!(
        ".globl _hash_final_finalized_ctx_msg\n_hash_final_finalized_ctx_msg:\n    .ascii {:?}\n",
        HASH_FINAL_FINALIZED_CTX_MSG
    ));
    out.push_str(&format!(
        ".globl _hash_copy_finalized_ctx_msg\n_hash_copy_finalized_ctx_msg:\n    .ascii {:?}\n",
        HASH_COPY_FINALIZED_CTX_MSG
    ));
    out.push_str(&format!(
        ".globl _mb_strlen_unknown_encoding_msg\n_mb_strlen_unknown_encoding_msg:\n    .ascii {:?}\n",
        MB_STRLEN_UNKNOWN_ENCODING_MSG
    ));
    out.push_str(".globl _mb_strlen_utf8_name\n_mb_strlen_utf8_name:\n    .asciz \"UTF-8\"\n");
    out.push_str(".globl _mb_strlen_utf8_alias\n_mb_strlen_utf8_alias:\n    .asciz \"UTF8\"\n");
    out.push_str(".globl _mb_strlen_utf32le_name\n_mb_strlen_utf32le_name:\n    .asciz \"UTF-32LE\"\n");
    out.push_str(".globl _mb_strlen_8bit_name\n_mb_strlen_8bit_name:\n    .asciz \"8bit\"\n");
    out.push_str(".globl _mb_strlen_binary_name\n_mb_strlen_binary_name:\n    .asciz \"binary\"\n");
    out.push_str(".globl _mb_strlen_7bit_name\n_mb_strlen_7bit_name:\n    .asciz \"7bit\"\n");
    // Fixed algorithm-name constants for md5()/sha1(): both route through the
    // same elephc_crypto_hash entry point as hash(), so __rt_md5 / __rt_sha1
    // load these literal names into the algorithm-name register pair before
    // reaching __rt_hash. NUL-terminated for safety, but the runtime passes the
    // explicit byte length (3 / 4) so elephc_crypto_hash never reads the NUL.
    out.push_str(".globl _md5_algo_name\n_md5_algo_name:\n    .asciz \"md5\"\n");
    out.push_str(".globl _sha1_algo_name\n_sha1_algo_name:\n    .asciz \"sha1\"\n");
    // Labelled name constants (`_hash_algo_N`) for hash_algos(): __rt_hash_algos_list
    // pushes each as a string element. The list is the single source of truth in
    // runtime::strings::hash_algos::HASH_ALGOS (kept in lockstep with elephc-crypto).
    for (i, name) in crate::codegen_support::runtime::strings::hash_algos::HASH_ALGOS
        .iter()
        .enumerate()
    {
        out.push_str(&format!(
            ".globl _hash_algo_{i}\n_hash_algo_{i}:\n    .asciz \"{name}\"\n"
        ));
    }
    for (label, message) in [
        ("_spl_dll_pop_empty_msg", "Can't pop from an empty datastructure"),
        ("_spl_dll_shift_empty_msg", "Can't shift from an empty datastructure"),
        ("_spl_dll_peek_empty_msg", "Can't peek at an empty datastructure"),
        (
            "_spl_dll_add_range_msg",
            "SplDoublyLinkedList::add(): Argument #1 ($index) is out of range",
        ),
        (
            "_spl_dll_offset_get_range_msg",
            "SplDoublyLinkedList::offsetGet(): Argument #1 ($index) is out of range",
        ),
        (
            "_spl_dll_offset_set_range_msg",
            "SplDoublyLinkedList::offsetSet(): Argument #1 ($index) is out of range",
        ),
        (
            "_spl_dll_offset_unset_range_msg",
            "SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) is out of range",
        ),
        (
            "_spl_dll_offset_exists_type_msg",
            "SplDoublyLinkedList::offsetExists(): Argument #1 ($index) must be of type int, non-int given",
        ),
        (
            "_spl_dll_offset_get_type_msg",
            "SplDoublyLinkedList::offsetGet(): Argument #1 ($index) must be of type int, non-int given",
        ),
        (
            "_spl_dll_offset_set_type_msg",
            "SplDoublyLinkedList::offsetSet(): Argument #1 ($index) must be of type ?int, non-int given",
        ),
        (
            "_spl_dll_offset_unset_type_msg",
            "SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) must be of type int, non-int given",
        ),
        (
            "_spl_fixed_construct_size_msg",
            "SplFixedArray::__construct(): Argument #1 ($size) must be greater than or equal to 0",
        ),
        (
            "_spl_fixed_set_size_msg",
            "SplFixedArray::setSize(): Argument #1 ($size) must be greater than or equal to 0",
        ),
        (
            "_spl_fixed_offset_type_msg",
            "Cannot access offset of type non-int on SplFixedArray",
        ),
        ("_spl_fixed_offset_range_msg", "Index invalid or out of range"),
        (
            "_spl_fixed_from_array_keys_msg",
            "array must contain only positive integer keys",
        ),
        (
            "_array_filter_mode_msg",
            "array_filter(): Argument #3 ($mode) must be one of ARRAY_FILTER_USE_VALUE, ARRAY_FILTER_USE_KEY, or ARRAY_FILTER_USE_BOTH.",
        ),
        (
            "_iterator_iterator_downcast_msg",
            "Class to downcast to not found or not base class or does not implement Traversable",
        ),
    ] {
        out.push_str(&format!(".globl {label}\n{label}:\n    .ascii {message:?}\n"));
    }
    out.push_str(".globl _uncaught_exc_msg\n_uncaught_exc_msg:\n    .ascii \"Fatal error: uncaught exception\\n\"\n");
    // PHP prefixes the report with a newline UNCONDITIONALLY — measured against 8.5 on a script
    // that writes nothing at all before throwing, where the output still begins with "\n".
    out.push_str(".globl _uncaught_exc_prefix\n_uncaught_exc_prefix:\n    .ascii \"\\nFatal error: Uncaught \"\n");
    out.push_str(".globl _uncaught_exc_sep\n_uncaught_exc_sep:\n    .ascii \": \"\n");
    out.push_str(".globl _uncaught_exc_in\n_uncaught_exc_in:\n    .ascii \" in \"\n");
    out.push_str(".globl _uncaught_exc_colon\n_uncaught_exc_colon:\n    .ascii \":\"\n");
    out.push_str(".globl _uncaught_exc_nl\n_uncaught_exc_nl:\n    .ascii \"\\n\"\n");
    // Printed only when `__rt_exception_matches` walks into the "metadata never emitted"
    // sentinel, which no correct build should reach — see that helper for why it aborts there
    // instead of answering "no match".
    out.push_str(&format!(
        ".globl {symbol}\n{symbol}:\n    .ascii {message:?}\n",
        symbol = crate::codegen_support::runtime::exceptions::ABSENT_MESSAGE_SYMBOL,
        message = crate::codegen_support::runtime::exceptions::ABSENT_MESSAGE,
    ));
    out.push_str(".globl _instanceof_target_type_msg\n_instanceof_target_type_msg:\n    .ascii \"Fatal error: Class name must be a valid object or a string\\n\"\n");
    // The composer copies exactly `STRING_OFFSET_PREFIX.len()` bytes from here, so the text
    // and the length it copies come from the same constant and cannot drift apart.
    out.push_str(&format!(
        ".globl _str_offset_warn_prefix\n_str_offset_warn_prefix:\n    .ascii \"{}\"\n",
        crate::codegen_support::runtime::objects::STRING_OFFSET_PREFIX
    ));
    out.push_str(".globl _diag_file_get_contents_failed_msg\n_diag_file_get_contents_failed_msg:\n    .ascii \"Warning: file_get_contents(): Failed to open stream\\n\"\n");
    out.push_str(".globl _diag_fopen_failed_msg\n_diag_fopen_failed_msg:\n    .ascii \"Warning: fopen(): Failed to open stream\\n\"\n");
    // Emitted FROM the constants that document them, not from a second copy of the text: the
    // wording and the reason for it live next to the helper that raises the warning, and a
    // literal here would drift from that the first time either is edited.
    for (symbol, message) in [
        ("_swr_bad_proto_msg", crate::codegen_support::runtime::io::BAD_PROTOCOL_WARNING),
        ("_swr_dup_proto_msg", crate::codegen_support::runtime::io::DUPLICATE_PROTOCOL_WARNING),
    ] {
        out.push_str(&format!(
            ".globl {symbol}\n{symbol}:\n    .ascii \"{}\"\n",
            message.replace('\n', "\\n")
        ));
    }
    // -- php-src's unreachable-seek warning fragments, shared with `__rt_file_get_contents_range` --
    // The helper derives its `__rt_concat` length immediates from the same table, so the bytes
    // here and the immediates there can never drift apart.
    for (label, message) in
        crate::codegen_support::runtime::io::FILE_GET_CONTENTS_SEEK_MESSAGES
    {
        out.push_str(&format!(".globl {label}\n{label}:\n    .ascii {message:?}\n"));
    }
    out.push_str(".globl _diag_define_already_defined_msg\n_diag_define_already_defined_msg:\n    .ascii \"Warning: define(): Constant already defined\\n\"\n");
    // The prefix ends with the OPENING quote php-src puts around the filter name; the
    // composer appends the name, the closing quote and the newline.
    out.push_str(".globl _diag_filter_missing_append_prefix\n_diag_filter_missing_append_prefix:\n    .ascii \"Warning: stream_filter_append(): Unable to locate filter \\\"\"\n");
    out.push_str(".globl _diag_filter_missing_prepend_prefix\n_diag_filter_missing_prepend_prefix:\n    .ascii \"Warning: stream_filter_prepend(): Unable to locate filter \\\"\"\n");
    out.push_str(".globl _diag_open_failed_middle\n_diag_open_failed_middle:\n    .ascii \"): Failed to open stream: \"\n");
    // The four sentences php-src's `php_stream_url_wrap_rfc2397` refuses a `data://` URI with, and
    // the one `php_stream_url_wrap_php` uses for a php:// target it does not recognise. Each is a
    // REASON — the composer supplies "Warning: fopen(<uri>): Failed to open stream: " and the
    // newline around it — so none of them carries punctuation of its own.
    for (label, reason) in crate::codegen_support::runtime::io::WRAPPER_REFUSAL_REASONS {
        out.push_str(&format!(".globl {label}\n{label}:\n    .ascii {reason:?}\n"));
    }
    // Whole line, newline included: php emits it with a direct `php_error_docref`, so it does not
    // pass through the "Failed to open stream: " composition at all.
    out.push_str(&format!(
        ".globl _diag_php_invalid_url\n_diag_php_invalid_url:\n    .ascii {:?}\n",
        crate::codegen_support::runtime::io::PHP_INVALID_URL_LINE
    ));
    // Closes php's quoting of the rejected fopen mode; `__rt_fopen_bad_mode_warning` writes the
    // opening backtick and the mode itself in front of it.
    out.push_str(&format!(
        ".globl _diag_mode_not_valid_tail\n_diag_mode_not_valid_tail:\n    .ascii {:?}\n",
        crate::codegen_support::runtime::io::BAD_MODE_TAIL
    ));
    // The fragments of php's two `php://fd/N` refusals. Neither is a plain REASON: both carry a
    // number that is only known while the program runs — the descriptor and its `errno` for the
    // first, `getdtablesize()` for the second — so `__rt_php_fd_open` writes them in pieces
    // rather than through the reason composer, and each fragment stops where a number begins.
    out.push_str(&format!(
        ".globl _diag_php_fd_dup_head\n_diag_php_fd_dup_head:\n    .ascii {:?}\n",
        crate::codegen_support::runtime::io::PHP_FD_DUP_HEAD
    ));
    out.push_str(&format!(
        ".globl _diag_php_fd_dup_middle\n_diag_php_fd_dup_middle:\n    .ascii {:?}\n",
        crate::codegen_support::runtime::io::PHP_FD_DUP_MIDDLE
    ));
    out.push_str(&format!(
        ".globl _diag_php_fd_dup_tail\n_diag_php_fd_dup_tail:\n    .ascii {:?}\n",
        crate::codegen_support::runtime::io::PHP_FD_DUP_TAIL
    ));
    out.push_str(&format!(
        ".globl _diag_php_fd_range_head\n_diag_php_fd_range_head:\n    .ascii {:?}\n",
        crate::codegen_support::runtime::io::PHP_FD_RANGE_HEAD
    ));
    out.push_str(".globl _diag_open_failed_fopen_prefix\n_diag_open_failed_fopen_prefix:\n    .ascii \"Warning: fopen(\"\n");
    out.push_str(".globl _diag_open_failed_fgc_prefix\n_diag_open_failed_fgc_prefix:\n    .ascii \"Warning: file_get_contents(\"\n");
    out.push_str(".globl _diag_open_failed_fpc_prefix\n_diag_open_failed_fpc_prefix:\n    .ascii \"Warning: file_put_contents(\"\n");
    out.push_str(".globl _diag_open_failed_file_prefix\n_diag_open_failed_file_prefix:\n    .ascii \"Warning: file(\"\n");
    out.push_str(".globl _diag_open_failed_readfile_prefix\n_diag_open_failed_readfile_prefix:\n    .ascii \"Warning: readfile(\"\n");
    // Bare callee names for the unknown-wrapper warning, which puts "Warning: " and "(): "
    // around the name itself rather than before an open parenthesis.
    out.push_str(".globl _uww_name_fopen\n_uww_name_fopen:\n    .ascii \"fopen\"\n");
    out.push_str(".globl _uww_name_fgc\n_uww_name_fgc:\n    .ascii \"file_get_contents\"\n");
    out.push_str(".globl _diag_csv_escape_deprecated_fgetcsv_msg\n_diag_csv_escape_deprecated_fgetcsv_msg:\n    .ascii \"Deprecated: fgetcsv(): the $escape parameter must be provided as its default value will change\\n\"\n");
    out.push_str(".globl _diag_csv_escape_deprecated_fputcsv_msg\n_diag_csv_escape_deprecated_fputcsv_msg:\n    .ascii \"Deprecated: fputcsv(): the $escape parameter must be provided as its default value will change\\n\"\n");
    out.push_str(".globl _diag_csv_escape_deprecated_str_getcsv_msg\n_diag_csv_escape_deprecated_str_getcsv_msg:\n    .ascii \"Deprecated: str_getcsv(): the $escape parameter must be provided as its default value will change\\n\"\n");
    out.push_str(".globl _diag_undefined_array_key_prefix\n_diag_undefined_array_key_prefix:\n    .ascii \"Warning: Undefined array key \"\n");
    out.push_str(".globl _diag_undefined_array_key_quote\n_diag_undefined_array_key_quote:\n    .ascii \"\\\"\"\n");
    out.push_str(".globl _diag_undefined_array_key_suffix\n_diag_undefined_array_key_suffix:\n    .ascii \"\\n\"\n");
    out.push_str(".globl _diag_array_offset_on_null\n_diag_array_offset_on_null:\n    .ascii \"Warning: Trying to access array offset on null\\n\"\n");
    // -- one complete message per foreach() argument type, shared with the helper emitter --
    // `__rt_warn_foreach_non_iterable` derives every `write()` length from the same table,
    // so the bytes here and the immediates there can never drift apart.
    for (label, message) in
        crate::codegen_support::runtime::arrays::FOREACH_NON_ITERABLE_MESSAGES
    {
        out.push_str(&format!(".globl {label}\n{label}:\n    .ascii {message:?}\n"));
    }
    // -- php-src's array_flip() skipped-entry warning, shared with the `__rt_hash_flip` emitter --
    // `__rt_hash_flip` derives its `write()` length from the same table, so the bytes here and
    // the immediate there can never drift apart.
    for (label, message) in crate::codegen_support::runtime::arrays::ARRAY_FLIP_SKIPPED_MESSAGES {
        out.push_str(&format!(".globl {label}\n{label}:\n    .ascii {message:?}\n"));
    }
    // -- php-src's array_count_values() skipped-entry warning, shared with its runtime emitter --
    // The emitter derives its `write()` length from the same table, so the bytes here and the
    // immediate there can never drift apart.
    for (label, message) in
        crate::codegen_support::runtime::arrays::ARRAY_COUNT_VALUES_SKIPPED_MESSAGES
    {
        out.push_str(&format!(".globl {label}\n{label}:\n    .ascii {message:?}\n"));
    }
    // -- PHP 8.5's NAN-to-bool coercion warning, shared with `__rt_warn_nan_coerced_bool` --
    // Emitted for every profile even though only 8.5 call sites reference it: the literal is
    // 50 bytes of `.data` and keeping it unconditional means the runtime `.data` layout does
    // not have to agree with the version gate that lives on the CALL sites.
    for (label, message) in
        crate::codegen_support::runtime::arrays::NAN_BOOL_COERCION_MESSAGES
    {
        out.push_str(&format!(".globl {label}\n{label}:\n    .ascii {message:?}\n"));
    }
    // -- PHP 8.5's NAN-to-string coercion warning, shared with `__rt_warn_nan_coerced_string` --
    // Unconditional for the same reason as the bool literal above: the `.data` layout must not
    // depend on the version gate that lives on the CALL site inside `__rt_ftoa`.
    for (label, message) in
        crate::codegen_support::runtime::strings::NAN_STRING_COERCION_MESSAGES
    {
        out.push_str(&format!(".globl {label}\n{label}:\n    .ascii {message:?}\n"));
    }
    out.push_str(".globl _fiber_msg_already_started\n_fiber_msg_already_started:\n    .ascii \"Cannot start a fiber that has already been started\"\n");
    out.push_str(".globl _fiber_msg_not_suspended\n_fiber_msg_not_suspended:\n    .ascii \"Cannot resume a fiber that is not suspended\"\n");
    out.push_str(".globl _fiber_msg_throw_not_suspended\n_fiber_msg_throw_not_suspended:\n    .ascii \"Cannot resume a fiber that is not suspended\"\n");
    out.push_str(".globl _fiber_msg_not_terminated\n_fiber_msg_not_terminated:\n    .ascii \"Cannot get fiber return value: The fiber has not returned\"\n");
    out.push_str(".globl _fiber_msg_suspend_outside\n_fiber_msg_suspend_outside:\n    .ascii \"Cannot suspend outside of a fiber\"\n");
    out.push_str(".globl _fiber_msg_suspend_unserialize\n_fiber_msg_suspend_unserialize:\n    .ascii \"Cannot suspend a fiber while unserialize() is active\"\n");
    out.push_str(".globl _fiber_msg_unsupported_callable\n_fiber_msg_unsupported_callable:\n    .ascii \"Fiber callable is not supported by this compiler\"\n");
    out.push_str(".globl _fiber_msg_stack_alloc_failed\n_fiber_msg_stack_alloc_failed:\n    .ascii \"Cannot allocate fiber stack\"\n");
    out.push_str(&emit_builtin_callable_data(target));
    out.push_str(&comm_directive("_gc_allocs", 8, target));
    out.push_str(&comm_directive("_gc_frees", 8, target));
    out.push_str(&comm_directive("_gc_live", 8, target));
    out.push_str(&comm_directive("_gc_peak", 8, target));
    out.push_str(&comm_directive("_cstr_buf", 4096, target));
    out.push_str(&comm_directive("_cstr_buf2", 4096, target));
    // `_eof_flags`, `_popen_files`, `_dir_handles` and `_glob_handles` used to live here
    // as fd-indexed tables. The generation-safe resource registry owns that state now,
    // reached through the opaque handle, so a reused descriptor number can no longer
    // inherit a closed stream's flags.
    // _stream_read_filters / _stream_write_filters: per-fd filter chain,
    // 2 slots per fd (slot 0 = first applied, slot 1 = second). A zero byte
    // means "no filter". 256 fds × 2 slots = 512 bytes each.
    out.push_str(&comm_directive("_stream_read_filters", 512, target));
    out.push_str(&comm_directive("_stream_write_filters", 512, target));
    out.push_str(&comm_directive("_stream_filter_buf", 65536, target));
    // 64KB scratch used by length-growing stream filters (convert.base64-encode,
    // convert.quoted-printable-encode). The filter encodes into the scratch and
    // then memcpy()s back into the caller's buffer, capping input at 49152 bytes
    // so the 4/3 base64 expansion still fits the scratch.
    out.push_str(&comm_directive("_stream_grow_scratch", 65536, target));
    out.push_str(&comm_directive("_zstream_handles", 2048, target));
    out.push_str(&comm_directive("_zlib_fwrite_fn", 8, target));
    out.push_str(&comm_directive("_zlib_close_fn", 8, target));
    out.push_str(&comm_directive("_phar_zlib_inflate_init2_fn", 8, target));
    out.push_str(&comm_directive("_phar_zlib_inflate_fn", 8, target));
    out.push_str(&comm_directive("_phar_zlib_inflate_end_fn", 8, target));
    out.push_str(".globl _zlib_version\n_zlib_version:\n    .asciz \"1\"\n");
    // bzip2.compress write-filter state: per-fd bz_stream pointer table
    // (_bzstream_handles, indexed by fd) plus the indirect fn-pointer slots the
    // shared runtime calls through so non-bzip2 programs never link -lbz2.
    out.push_str(&comm_directive("_bzstream_handles", 2048, target));
    out.push_str(&comm_directive("_bz2_fwrite_fn", 8, target));
    out.push_str(&comm_directive("_bz2_close_fn", 8, target));
    out.push_str(&comm_directive("_phar_bz2_decompress_fn", 8, target));
    // convert.iconv.* WRITE-filter state: per-fd iconv_t descriptor table
    // (_iconv_handles) plus the indirect fn-pointer slots the shared runtime
    // calls through so it never names libc iconv (which needs -liconv on macOS).
    out.push_str(&comm_directive("_iconv_handles", 2048, target));
    out.push_str(&comm_directive("_iconv_fwrite_fn", 8, target));
    out.push_str(&comm_directive("_iconv_close_fn", 8, target));
    out.push_str(&comm_directive("_ftp_resp_buf", 4096, target));
    out.push_str(&comm_directive("_ftp_data_addr", 64, target));
    // _ftp_use_tls: set to 1 by fopen("ftps://...") before __rt_ftp_open is
    // invoked. The handshake helper interprets it as "perform AUTH TLS on the
    // control connection, PBSZ 0 + PROT P after USER/PASS, and elephc-tls-
    // attach the PASV data connection". Reset to 0 at the end of __rt_ftp_open
    // so subsequent plain ftp:// opens are not contaminated.
    out.push_str(&comm_directive("_ftp_use_tls", 8, target));
    out.push_str(&comm_directive("_http_resp_buf", 1048576, target));
    // https:// goes through indirect function pointers so only programs that
    // actually open https URLs reference elephc-tls (and pull in -lelephc_tls
    // at link time); other programs keep the runtime libc-only.
    out.push_str(&comm_directive("_elephc_tls_connect_fn", 8, target));
    // _elephc_tls_connect_insecure_fn: same shape as _elephc_tls_connect_fn
    // but dispatched when the caller has set ssl.verify_peer = false on the
    // stream context. The runtime picks one over the other at https_open
    // time so non-TLS programs still don't link elephc-tls.
    out.push_str(&comm_directive("_elephc_tls_connect_insecure_fn", 8, target));
    // _elephc_tls_connect_cafile_fn: dispatched when the caller has set
    // ssl.cafile on the stream context. Same late-binding pattern; takes two
    // extra args (cafile path ptr/len) that the secure/insecure variants ignore.
    out.push_str(&comm_directive("_elephc_tls_connect_cafile_fn", 8, target));
    // _elephc_tls_connect_capath_fn / _peer_name_fn: dispatched for ssl.capath
    // (a directory of CA certs) and ssl.peer_name (verify the cert for a name
    // other than the connection host). Same late-binding/extra-args pattern.
    out.push_str(&comm_directive("_elephc_tls_connect_capath_fn", 8, target));
    out.push_str(&comm_directive("_elephc_tls_connect_peer_name_fn", 8, target));
    out.push_str(&comm_directive("_elephc_tls_write_fn", 8, target));
    out.push_str(&comm_directive("_elephc_tls_read_fn", 8, target));
    out.push_str(&comm_directive("_elephc_tls_close_fn", 8, target));
    // _elephc_tls_peer_fingerprint_matches_fn: dispatched for ssl.peer_fingerprint
    // once the handshake has produced a peer certificate. A null slot means the
    // bridge is absent, and the pin then fails CLOSED — a fingerprint that cannot
    // be checked must never look like one that matched.
    out.push_str(&comm_directive(
        "_elephc_tls_peer_fingerprint_matches_fn",
        8,
        target,
    ));
    // _elephc_tls_attach_fd_fn: indirect pointer to elephc_tls_attach_fd,
    // used by stream_socket_enable_crypto to promote an existing TCP fd to
    // a TLS session without re-establishing the TCP connection. Same
    // late-binding pattern as the other tls fn slots so non-TLS programs
    // do not pull in elephc-tls at link time.
    out.push_str(&comm_directive("_elephc_tls_attach_fd_fn", 8, target));
    // Non-verifying attach, selected when the stream context sets
    // ssl.verify_peer to false.
    out.push_str(&comm_directive("_elephc_tls_attach_fd_insecure_fn", 8, target));
    // _elephc_tls_attach_fd_client_cert_fn / _elephc_tls_connect_client_cert_fn:
    // mutual-TLS variants dispatched when the stream context carries both
    // ssl.local_cert and ssl.local_pk. The attach variant is used by
    // stream_socket_enable_crypto; both take the extra cert/key path ptr/len
    // pairs that the non-client-cert variants ignore. Same late-binding pattern.
    out.push_str(&comm_directive("_elephc_tls_attach_fd_client_cert_fn", 8, target));
    out.push_str(&comm_directive("_elephc_tls_connect_client_cert_fn", 8, target));
    // _elephc_crypto_hash_fn: indirect pointer to elephc_crypto_hash, published
    // only at a hash() call site so the shared runtime __rt_hash can call through
    // it without the runtime itself naming elephc-crypto. Programs that never
    // call hash() leave the slot null and do not pull in -lelephc_crypto.
    out.push_str(&comm_directive("_elephc_crypto_hash_fn", 8, target));
    // _elephc_crypto_hmac_fn: indirect pointer to elephc_crypto_hmac, published
    // only at a hash_hmac() call site so the shared runtime __rt_hash_hmac can call
    // through it without the runtime itself naming elephc-crypto. Programs that never
    // call hash_hmac() leave the slot null and do not pull in -lelephc_crypto.
    out.push_str(&comm_directive("_elephc_crypto_hmac_fn", 8, target));
    // Incremental HashContext entry slots, published by hash_init/update/final/copy.
    out.push_str(&comm_directive("_elephc_crypto_init_fn", 8, target));
    out.push_str(&comm_directive("_elephc_crypto_update_fn", 8, target));
    out.push_str(&comm_directive("_elephc_crypto_final_fn", 8, target));
    out.push_str(&comm_directive("_elephc_crypto_clone_fn", 8, target));
    // _elephc_crypto_free_fn: indirect pointer to elephc_crypto_free, published
    // at hash_init/hash_copy call sites and used by __rt_hash_ctx_free so the
    // shared runtime can release unfinalized HashContext handles without naming
    // elephc-crypto directly.
    out.push_str(&comm_directive("_elephc_crypto_free_fn", 8, target));
    // _elephc_crypto_is_finalized_fn: indirect pointer to elephc_crypto_is_finalized.
    // __rt_hash_update / __rt_hash_final / __rt_hash_copy ask through it whether the
    // incoming context was already consumed by a previous hash_final(), which is the
    // condition PHP 8 answers with a TypeError. A null slot means the bridge is not
    // linked, in which case the guards skip the question exactly like every other
    // elephc-crypto call in this family.
    out.push_str(&comm_directive("_elephc_crypto_is_finalized_fn", 8, target));
    // Symmetric-cipher bridge slots. OpenSSL call sites publish these separately
    // from the hash family so hash-only programs never reference cipher symbols.
    out.push_str(&comm_directive(
        "_elephc_crypto_cipher_iv_length_fn",
        8,
        target,
    ));
    out.push_str(&comm_directive(
        "_elephc_crypto_cipher_methods_fn",
        8,
        target,
    ));
    out.push_str(&comm_directive("_elephc_crypto_encrypt_fn", 8, target));
    out.push_str(&comm_directive("_elephc_crypto_decrypt_fn", 8, target));
    // BCMath bridge slots are published only by `bc*` call sites. Shared runtime
    // helpers call through these slots so unrelated programs never reference the
    // optional elephc-bcmath archive.
    for slot in [
        "_elephc_bcmath_add_fn",
        "_elephc_bcmath_sub_fn",
        "_elephc_bcmath_mul_fn",
        "_elephc_bcmath_div_fn",
        "_elephc_bcmath_mod_fn",
        "_elephc_bcmath_divmod_fn",
        "_elephc_bcmath_pow_fn",
        "_elephc_bcmath_powmod_fn",
        "_elephc_bcmath_sqrt_fn",
        "_elephc_bcmath_comp_fn",
        "_elephc_bcmath_get_scale_fn",
        "_elephc_bcmath_set_scale_fn",
        "_elephc_bcmath_ceil_fn",
        "_elephc_bcmath_floor_fn",
        "_elephc_bcmath_round_fn",
        "_elephc_bcmath_last_error_fn",
        "_elephc_bcmath_free_fn",
    ] {
        out.push_str(&comm_directive(slot, 8, target));
    }
    // _elephc_phar_extract_url_fn: indirect pointer to the elephc-phar bridge
    // reader. Dynamic phar:// paths publish it before calling the runtime
    // reader; literal phar:// paths are still decoded at compile time.
    out.push_str(".p2align 3\n.globl _elephc_phar_extract_url_fn\n_elephc_phar_extract_url_fn:\n    .quad 0\n");
    // _elephc_phar_put_entry_fn: indirect pointer to the elephc-phar native
    // writer bridge. phar:// write paths publish it so finalize can preserve
    // existing native PHAR entries instead of regenerating a single-entry file.
    out.push_str(".p2align 3\n.globl _elephc_phar_put_entry_fn\n_elephc_phar_put_entry_fn:\n    .quad 0\n");
    // _elephc_phar_put_url_fn: indirect pointer to the elephc-phar native
    // writer bridge for runtime-built phar:// file_put_contents() URLs.
    out.push_str(".p2align 3\n.globl _elephc_phar_put_url_fn\n_elephc_phar_put_url_fn:\n    .quad 0\n");
    // _elephc_phar_delete_url_fn: indirect pointer to the elephc-phar entry
    // deletion bridge used by unlink("phar://...") and Phar::offsetUnset().
    out.push_str(".p2align 3\n.globl _elephc_phar_delete_url_fn\n_elephc_phar_delete_url_fn:\n    .quad 0\n");
    // _elephc_phar_set_compression_fn: indirect pointer to the elephc-phar
    // native-PHAR compression-control bridge used by Phar::compressFiles().
    out.push_str(".p2align 3\n.globl _elephc_phar_set_compression_fn\n_elephc_phar_set_compression_fn:\n    .quad 0\n");
    // _elephc_phar_list_entries_fn: indirect pointer to the elephc-phar archive
    // listing bridge used by Phar/PharData constructors to seed iteration.
    out.push_str(".p2align 3\n.globl _elephc_phar_list_entries_fn\n_elephc_phar_list_entries_fn:\n    .quad 0\n");
    // _elephc_zip_stat_entries_fn: indirect pointer to the elephc-phar ZIP
    // central-directory stat bridge used by ZipArchive::open() to seed its entries.
    out.push_str(".p2align 3\n.globl _elephc_zip_stat_entries_fn\n_elephc_zip_stat_entries_fn:\n    .quad 0\n");
    // _elephc_phar_get/set_metadata_fn and _elephc_phar_get/set_stub_fn: indirect
    // pointers to the elephc-phar global-metadata and stub read/write bridges used
    // by the Phar/PharData metadata and stub accessors.
    out.push_str(".p2align 3\n.globl _elephc_phar_get_metadata_fn\n_elephc_phar_get_metadata_fn:\n    .quad 0\n");
    out.push_str(".p2align 3\n.globl _elephc_phar_set_metadata_fn\n_elephc_phar_set_metadata_fn:\n    .quad 0\n");
    out.push_str(".p2align 3\n.globl _elephc_phar_get_stub_fn\n_elephc_phar_get_stub_fn:\n    .quad 0\n");
    out.push_str(".p2align 3\n.globl _elephc_phar_set_stub_fn\n_elephc_phar_set_stub_fn:\n    .quad 0\n");
    // Per-file (PharFileInfo) metadata read/write bridges.
    out.push_str(".p2align 3\n.globl _elephc_phar_get_file_metadata_fn\n_elephc_phar_get_file_metadata_fn:\n    .quad 0\n");
    out.push_str(".p2align 3\n.globl _elephc_phar_set_file_metadata_fn\n_elephc_phar_set_file_metadata_fn:\n    .quad 0\n");
    // Whole-archive (tar) compression bridges.
    out.push_str(".p2align 3\n.globl _elephc_phar_gzip_archive_fn\n_elephc_phar_gzip_archive_fn:\n    .quad 0\n");
    out.push_str(".p2align 3\n.globl _elephc_phar_bzip2_archive_fn\n_elephc_phar_bzip2_archive_fn:\n    .quad 0\n");
    out.push_str(".p2align 3\n.globl _elephc_phar_decompress_archive_fn\n_elephc_phar_decompress_archive_fn:\n    .quad 0\n");
    // PHAR signature bridges (OpenSSL RSA + hash algorithms + signature read).
    out.push_str(".p2align 3\n.globl _elephc_phar_sign_openssl_fn\n_elephc_phar_sign_openssl_fn:\n    .quad 0\n");
    out.push_str(".p2align 3\n.globl _elephc_phar_sign_hash_fn\n_elephc_phar_sign_hash_fn:\n    .quad 0\n");
    out.push_str(".p2align 3\n.globl _elephc_phar_get_signature_hash_fn\n_elephc_phar_get_signature_hash_fn:\n    .quad 0\n");
    out.push_str(".p2align 3\n.globl _elephc_phar_get_signature_type_fn\n_elephc_phar_get_signature_type_fn:\n    .quad 0\n");
    // PHAR ZipCrypto password bridge (read encrypted ZIP entries).
    out.push_str(".p2align 3\n.globl _elephc_phar_set_zip_password_fn\n_elephc_phar_set_zip_password_fn:\n    .quad 0\n");
    // _elephc_phar_stream_*_fn: indirect pointers to the elephc-phar buffered
    // write-stream bridge. These allow multiple phar:// write descriptors to
    // stay open at once while the old assembly single-entry writer remains as
    // a fallback when the bridge is not linked.
    out.push_str(".p2align 3\n.globl _elephc_phar_stream_open_entry_fn\n_elephc_phar_stream_open_entry_fn:\n    .quad 0\n");
    out.push_str(".p2align 3\n.globl _elephc_phar_stream_open_url_fn\n_elephc_phar_stream_open_url_fn:\n    .quad 0\n");
    out.push_str(".p2align 3\n.globl _elephc_phar_stream_append_fn\n_elephc_phar_stream_append_fn:\n    .quad 0\n");
    out.push_str(".p2align 3\n.globl _elephc_phar_stream_finalize_fn\n_elephc_phar_stream_finalize_fn:\n    .quad 0\n");
    // _phar_extract_len: output-length scratch written by elephc_phar_extract_url
    // and consumed immediately by __rt_phar_read_entry before __rt_data_stream
    // copies the bytes into a temp-file-backed descriptor.
    out.push_str(".p2align 3\n.globl _phar_extract_len\n_phar_extract_len:\n    .quad 0\n");
    // _phar_list_len: output-length scratch written by elephc_phar_list_entries
    // and consumed immediately while expanding serialized names into an array.
    out.push_str(".p2align 3\n.globl _phar_list_len\n_phar_list_len:\n    .quad 0\n");
    // _zip_stat_len: the same output-length scratch for elephc_zip_stat_entries. It is
    // its OWN word rather than a shared one: a ZipArchive method body can list entries
    // through both bridges, and one length must not overwrite the other's.
    out.push_str(".p2align 3\n.globl _zip_stat_len\n_zip_stat_len:\n    .quad 0\n");
    // FTP's TLS sessions. PHP-visible streams keep theirs on the StreamState, reached
    // through the opaque handle; FTP's control and data sockets are INTERNAL — they are
    // never exposed as PHP resources, so adopting them into the registry would mint
    // resource ids and shift the php-src-aligned numbering. The `__rt_ftp_*` helpers are
    // synchronous, so at most one control and one data connection are live at a time and
    // two words suffice. Unlike the fd-indexed `_tls_sessions` table these replace, there
    // is no 256 ceiling and a reused descriptor number cannot inherit a previous session.
    // `_stream_chunk_size` and `_stream_connect_host` moved onto the StreamState for the
    // same reason.
    out.push_str(&comm_directive("_ftp_tls_control", 8, target));
    out.push_str(&comm_directive("_ftp_tls_data", 8, target));
    // _stream_notification_callback: the callable descriptor pointer for the
    // stream context's `notification` option, captured at codegen time by
    // stream_context_create / stream_context_set_params. __rt_http_open fires
    // it at the CONNECT, COMPLETED, and FAILURE transfer milestones. Zero when
    // no notification callback is registered (the fire shim is then a no-op).
    out.push_str(&comm_directive("_stream_notification_callback", 8, target));
    // _tls_peer_name_default: hardcoded peer-name buffer used as the SNI
    // hint when stream_socket_enable_crypto is called without a context
    // peer_name. v1 limitation — production TLS needs real peer-name
    // passing via the stream context (deferred).
    out.push_str(
        ".globl _tls_peer_name_default\n_tls_peer_name_default:\n    .ascii \"localhost\"\n",
    );
    // Key literals used by __rt_get_string_context_option for the
    // _stream_context_options["ssl"]["peer_name"] lookup.
    out.push_str(".globl _ssl_key_str\n_ssl_key_str:\n    .ascii \"ssl\"\n");
    out.push_str(
        ".globl _ssl_peer_name_key_str\n_ssl_peer_name_key_str:\n    .ascii \"peer_name\"\n",
    );
    out.push_str(
        ".globl _ssl_verify_peer_key_str\n_ssl_verify_peer_key_str:\n    .ascii \"verify_peer\"\n",
    );
    out.push_str(
        ".globl _ssl_cafile_key_str\n_ssl_cafile_key_str:\n    .ascii \"cafile\"\n",
    );
    out.push_str(
        ".globl _ssl_capath_key_str\n_ssl_capath_key_str:\n    .ascii \"capath\"\n",
    );
    // ssl.local_cert / ssl.local_pk: the client-certificate chain and private
    // key paths for mutual TLS, consumed by stream_socket_enable_crypto.
    out.push_str(
        ".globl _ssl_local_cert_key_str\n_ssl_local_cert_key_str:\n    .ascii \"local_cert\"\n",
    );
    out.push_str(
        ".globl _ssl_local_pk_key_str\n_ssl_local_pk_key_str:\n    .ascii \"local_pk\"\n",
    );
    // (_ssl_peer_name_key_str is already defined above; the TLS attach path reads
    // ssl.peer_name through __rt_get_string_context_option like every other string option)
    out.push_str(
        ".globl _ssl_allow_self_signed_key_str\n_ssl_allow_self_signed_key_str:\n    .ascii \"allow_self_signed\"\n",
    );
    out.push_str(
        ".globl _ssl_verify_peer_name_key_str\n_ssl_verify_peer_name_key_str:\n    .ascii \"verify_peer_name\"\n",
    );
    out.push_str(
        ".globl _ssl_peer_fingerprint_key_str\n_ssl_peer_fingerprint_key_str:\n    .ascii \"peer_fingerprint\"\n",
    );
    // php-src's `php_openssl_capture_peer_certs` prints this exact sentence through
    // `php_error_docref` when the pin does not match. elephc's runtime does not know
    // which builtin is on the stack, so the `<callee>(): ` prefix php puts in front
    // of it is missing; the sentence itself is verbatim.
    out.push_str(&format!(
        ".globl _diag_peer_fingerprint_mismatch\n_diag_peer_fingerprint_mismatch:\n    .ascii {:?}\n",
        crate::codegen_support::runtime::io::PEER_FINGERPRINT_MISMATCH_LINE
    ));
    // Key literals + request fragments used by __rt_http_build_request.
    out.push_str(".globl _http_key_str\n_http_key_str:\n    .ascii \"http\"\n");
    out.push_str(
        ".globl _http_method_key_str\n_http_method_key_str:\n    .ascii \"method\"\n",
    );
    out.push_str(
        ".globl _http_header_key_str\n_http_header_key_str:\n    .ascii \"header\"\n",
    );
    out.push_str(
        ".globl _http_content_key_str\n_http_content_key_str:\n    .ascii \"content\"\n",
    );
    // "Content-Length: " — 16 bytes, written before the numeric length
    // when context supplies a body. The numeric length comes from
    // __rt_itoa, followed by a CRLF before the Connection header.
    out.push_str(
        ".globl _http_content_length_prefix\n_http_content_length_prefix:\n    .ascii \"Content-Length: \"\n",
    );
    // Phase B HTTP-context option keys + header prefixes used by
    // __rt_http_build_request when stream_context_set_option(... 'http' ...)
    // provides the corresponding value.
    out.push_str(
        ".globl _http_user_agent_key_str\n_http_user_agent_key_str:\n    .ascii \"user_agent\"\n",
    );
    out.push_str(
        ".globl _http_user_agent_prefix\n_http_user_agent_prefix:\n    .ascii \"User-Agent: \"\n",
    );
    out.push_str(
        ".globl _http_protocol_version_key_str\n_http_protocol_version_key_str:\n    .ascii \"protocol_version\"\n",
    );
    // 17-byte " HTTP/1.1\r\nHost: " variant used when [http][protocol_version]
    // is the literal string "1.1".
    out.push_str(
        ".globl _http_version_host_11\n_http_version_host_11:\n    .ascii \" HTTP/1.1\\r\\nHost: \"\n",
    );
    out.push_str(
        ".globl _http_proxy_key_str\n_http_proxy_key_str:\n    .ascii \"proxy\"\n",
    );
    // Socket-wrapper context option keys read by stream_socket_client /
    // stream_socket_server before / after their respective syscalls.
    // _empty_str: a guaranteed-readable 1-byte buffer used as the pointer for a
    // zero-length string null-fallback (out-of-bounds indexed read / assoc miss
    // on a Str-typed array). len 0 means no bytes are ever read; the valid
    // pointer keeps any echo/strlen path that still loads the pointer safe.
    out.push_str(&comm_directive("_empty_str", 1, target));
    // _url_stat_matched: set to 1 by __rt_user_wrapper_url_stat when a path's
    // scheme matches a registered userspace wrapper, 0 otherwise. The path-based
    // stat builtins (file_exists/is_file/filesize) read it after the call to
    // decide between the wrapper's url_stat() result and the real filesystem.
    out.push_str(&comm_directive("_url_stat_matched", 1, target));
    out.push_str(
        ".globl _socket_key_str\n_socket_key_str:\n    .ascii \"socket\"\n",
    );
    out.push_str(
        ".globl _socket_tcp_nodelay_key_str\n_socket_tcp_nodelay_key_str:\n    .ascii \"tcp_nodelay\"\n",
    );
    out.push_str(
        ".globl _socket_so_reuseport_key_str\n_socket_so_reuseport_key_str:\n    .ascii \"so_reuseport\"\n",
    );
    out.push_str(
        ".globl _socket_so_broadcast_key_str\n_socket_so_broadcast_key_str:\n    .ascii \"so_broadcast\"\n",
    );
    out.push_str(
        ".globl _socket_backlog_key_str\n_socket_backlog_key_str:\n    .ascii \"backlog\"\n",
    );
    out.push_str(
        ".globl _socket_ipv6_v6only_key_str\n_socket_ipv6_v6only_key_str:\n    .ascii \"ipv6_v6only\"\n",
    );
    out.push_str(
        ".globl _socket_bindto_key_str\n_socket_bindto_key_str:\n    .ascii \"bindto\"\n",
    );
    // "http://" — used as the scheme prefix when [http][request_fulluri] is truthy
    out.push_str(
        ".globl _http_scheme_prefix\n_http_scheme_prefix:\n    .ascii \"http://\"\n",
    );
    // Active HTTP context options written by __rt_http_build_request and
    // read by __rt_http_open. Lets the build-side (which performs the
    // context lookups) communicate enforcement-relevant values to the
    // socket-side without needing extra args.
    //   _http_active_ignore_errors : 1 = silently return body on 4xx/5xx;
    //                                0 = fail-open behavior (default in PHP).
    //   _http_active_max_redirects : count of remaining hops for
    //                                follow_location loops (0 disables).
    //   _http_active_timeout_set   : 1 = [http][timeout] was present, so the
    //                                read must not block past the deadline;
    //                                0 = no deadline (PHP's default).
    //   _http_active_timeout_seconds / _http_active_timeout_usec : the deadline
    //                                split into a `timeval`. PHP documents the
    //                                option as a FLOAT, so the sub-second part
    //                                has to survive as microseconds.
    out.push_str(&comm_directive("_http_active_ignore_errors", 8, target));
    out.push_str(&comm_directive("_http_active_max_redirects", 8, target));
    out.push_str(&comm_directive("_http_active_timeout_set", 8, target));
    out.push_str(&comm_directive("_http_active_timeout_seconds", 8, target));
    out.push_str(&comm_directive("_http_active_timeout_usec", 8, target));
    // Proxy override for __rt_http_open: when non-zero, used as the TCP
    // connect target instead of the host extracted from the URL. Value
    // shape is "tcp://proxyhost:port" — the same format
    // __rt_stream_socket_client expects.
    out.push_str(&comm_directive("_http_active_proxy_ptr", 8, target));
    out.push_str(&comm_directive("_http_active_proxy_len", 8, target));
    // Host info written by __rt_http_build_request and consumed by
    // __rt_http_open when [http][follow_location] triggers an internal
    // redirect — we rebuild the request with the saved host + the
    // Location-header path.
    out.push_str(&comm_directive("_http_active_host_ptr", 8, target));
    out.push_str(&comm_directive("_http_active_host_len", 8, target));
    // 2 KiB scratch for the Location header's path component on
    // relative redirects (covers the vast majority of API redirects).
    out.push_str(&comm_directive("_http_redirect_path_buf", 2048, target));
    out.push_str(&comm_directive("_http_redirect_path_len", 8, target));
    out.push_str(
        ".globl _http_request_fulluri_key_str\n_http_request_fulluri_key_str:\n    .ascii \"request_fulluri\"\n",
    );
    out.push_str(
        ".globl _http_follow_location_key_str\n_http_follow_location_key_str:\n    .ascii \"follow_location\"\n",
    );
    out.push_str(
        ".globl _http_max_redirects_key_str\n_http_max_redirects_key_str:\n    .ascii \"max_redirects\"\n",
    );
    out.push_str(
        ".globl _http_ignore_errors_key_str\n_http_ignore_errors_key_str:\n    .ascii \"ignore_errors\"\n",
    );
    out.push_str(
        ".globl _http_timeout_key_str\n_http_timeout_key_str:\n    .ascii \"timeout\"\n",
    );
    // The PHAR stub terminator (`__HALT_COMPILER();`, 18 bytes) that
    // __rt_phar_read_entry scans for at runtime to locate the manifest start.
    out.push_str(
        ".globl _phar_halt_magic\n_phar_halt_magic:\n    .ascii \"__HALT_COMPILER();\"\n",
    );
    // FTP context keys + command fragments used by __rt_ftp_open when
    // ['ftp']['resume_pos'] is set in the active stream context. v1
    // limitation: the value is stored as a string by stream_context_set_option,
    // so callers pass `'1024'` rather than `1024`.
    out.push_str(".globl _ftp_key_str\n_ftp_key_str:\n    .ascii \"ftp\"\n");
    out.push_str(
        ".globl _ftp_resume_pos_key_str\n_ftp_resume_pos_key_str:\n    .ascii \"resume_pos\"\n",
    );
    out.push_str(".globl _ftp_rest_prefix\n_ftp_rest_prefix:\n    .ascii \"REST \"\n");
    // 64-byte scratch for the dynamically built REST command. The largest
    // PHP int (19 ascii digits) + "REST " (5) + "\r\n" (2) = 26 bytes, so
    // 64 leaves generous headroom for future extensions (auth, custom
    // commands).
    out.push_str(&comm_directive("_ftp_cmd_scratch", 64, target));

    // Bucket-brigade property keys used by __rt_user_filter_brigade_invoke
    // to build and walk brigade-shaped argument data when the user's
    // filter() method uses the PHP-canonical 4-arg signature.
    out.push_str(".globl _brigade_buckets_key\n_brigade_buckets_key:\n    .ascii \"_buckets\"\n");
    out.push_str(".globl _brigade_data_key\n_brigade_data_key:\n    .ascii \"data\"\n");
    out.push_str(".globl _brigade_datalen_key\n_brigade_datalen_key:\n    .ascii \"datalen\"\n");
    out.push_str(".globl _http_default_method\n_http_default_method:\n    .ascii \"GET\"\n");
    // " HTTP/1.0\r\nHost: " — 17 bytes (space + version + CRLF + Host
    // header prefix). Inserted between the path and the host literal.
    out.push_str(
        ".globl _http_version_host\n_http_version_host:\n    .ascii \" HTTP/1.0\\r\\nHost: \"\n",
    );
    // "\r\n" — 2 bytes, CRLF separator written after the Host value
    // (and again after each context-supplied header line).
    out.push_str(".globl _http_crlf\n_http_crlf:\n    .ascii \"\\r\\n\"\n");
    // "Connection: close\r\n\r\n" — 21 bytes Connection header + blank
    // line separator that ends the request headers section.
    out.push_str(
        ".globl _http_trailer\n_http_trailer:\n    .ascii \"Connection: close\\r\\n\\r\\n\"\n",
    );
    // _http_req_scratch: 8 KB buffer for the dynamically-built HTTP/1.0
    // request. Comfortable headroom over (method 16 + path 4 KB + host
    // 253 + boilerplate 80) while keeping the BSS small. Populated by
    // `__rt_http_build_request` and consumed by `__rt_http_open` via
    // the http_stream lowering when context options can override the
    // default method.
    out.push_str(&comm_directive("_http_req_scratch", 8192, target));
    out.push_str(&comm_directive("_fgc_url_addr", 512, target));
    out.push_str(&comm_directive("_fgc_url_retr", 2048, target));
    out.push_str(".globl _fgc_url_slash\n_fgc_url_slash:\n    .ascii \"/\"\n");
    out.push_str(&comm_directive("_https_resp_buf", 1048576, target));
    out.push_str(&comm_directive("_fsockopen_addr", 512, target));
    // _fsockopen_uri_ptr/_len: the slice of `_fsockopen_addr` php would have handed to
    // `php_stream_xport_create` — the hostname plus `:port` when a port was given, WITHOUT the
    // `tcp://` prefix elephc adds for a schemeless host. That slice is what php records as the
    // stream's `uri` and what names its transport, so the two are published here for
    // `__rt_stream_record_fsockopen_meta` to read once the descriptor has been boxed.
    // The built-in filter parameter names, and the four words `__rt_asf_params_load` publishes for
    // the encoders. `_filter_break_lf` is php's default `line-break-chars`.
    out.push_str(".globl _filter_key_line_length\n_filter_key_line_length:\n    .ascii \"line-length\"\n");
    out.push_str(".globl _filter_key_line_break\n_filter_key_line_break:\n    .ascii \"line-break-chars\"\n");
    out.push_str(".globl _filter_key_binary\n_filter_key_binary:\n    .ascii \"binary\"\n");
    // php's default `line-break-chars` is CRLF, not a lone newline: measured on `php -n` 8.5.6,
    // `["line-length" => 8]` over "hello world" answers 18 bytes, `aGVsbG8g\r\nd29ybGQ=`.
    out.push_str(".globl _filter_break_crlf\n_filter_break_crlf:\n    .ascii \"\\r\\n\"\n");
    for (symbol, text) in [
        ("_diag_fp_invalid_append", FILTER_PARAM_INVALID_APPEND_HEAD),
        ("_diag_fp_invalid_prepend", FILTER_PARAM_INVALID_PREPEND_HEAD),
        ("_diag_fp_invalid_tail", FILTER_PARAM_INVALID_TAIL),
        ("_diag_fp_create_append", FILTER_PARAM_CREATE_APPEND_HEAD),
        ("_diag_fp_create_prepend", FILTER_PARAM_CREATE_PREPEND_HEAD),
        ("_diag_fp_create_tail", FILTER_PARAM_CREATE_TAIL),
    ] {
        out.push_str(&format!(
            ".globl {symbol}\n{symbol}:\n    .ascii \"{}\"\n",
            text.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
        ));
    }
    // _user_filter_current_stream: the stream a `filter()` call is running FOR, or zero outside
    // one. php publishes `$this->stream` for the DURATION of each call and nowhere else — measured
    // on `php -n` 8.5.6 it is UNSET in `onCreate()`, a live resource in `filter()`, and NULL again
    // in `onClose()` — so the handle travels here rather than being retained on the instance.
    // _zlib_flush_fn: the address of the inline `Z_SYNC_FLUSH` helper the deflate attachment
    // publishes, or zero when no such filter was ever attached. `fflush()` calls through it, which
    // is what keeps libz pay-for-use: the helper is part of the attachment, so a program that
    // never attaches one never references `deflate` at all.
    out.push_str(&comm_directive("_zlib_flush_fn", 8, target));
    out.push_str(&comm_directive("_user_filter_current_stream", 8, target));
    out.push_str(&comm_directive("_asf_line_length", 8, target));
    out.push_str(&comm_directive("_asf_break_ptr", 8, target));
    out.push_str(&comm_directive("_asf_break_len", 8, target));
    out.push_str(&comm_directive("_asf_binary", 8, target));
    out.push_str(&comm_directive("_fsockopen_uri_ptr", 8, target));
    out.push_str(&comm_directive("_fsockopen_uri_len", 8, target));
    // _user_wrappers_ptr: heap base of the scheme→class registration table, or
    // zero before the first registration. Each entry is 32 bytes
    // (protocol_ptr/len + class_ptr/len); a slot is free when protocol_ptr is
    // null. PHP imposes no limit on registered wrappers, so the table grows by
    // doubling through `__rt_user_wrappers_reserve` instead of exposing a fixed
    // capacity — Gate 1 forbids PHP-visible fixed capacities.
    crate::codegen_support::runtime::io::emit_builtin_filter_table(&mut out);
    crate::codegen_support::runtime::io::emit_builtin_wrapper_table(&mut out, target);
    out.push_str(&comm_directive("_user_wrappers_ptr", 8, target));
    // _user_wrappers_cap: number of allocated 32-byte slots, zero until the
    // table is first reserved. Every scan bounds itself on this value.
    out.push_str(&comm_directive("_user_wrappers_cap", 8, target));
    // Registration flags are definition-scoped and copied into each StreamState
    // at open time, so later unregister/reregister operations cannot mutate a
    // live stream instance's STREAM_IS_URL behavior. Grown alongside
    // _user_wrappers_ptr so slot indices stay aligned between the two.
    out.push_str(&comm_directive("_user_wrapper_flags_ptr", 8, target));
    // _user_wrapper_handles_ptr: heap base of the active stream-handle table, or
    // zero before the first wrapper stream is opened. Each 8-byte slot stores the
    // wrapper object pointer keyed by synthetic fd `USER_WRAPPER_FD_BASE +
    // slot_index`; a slot is free when the stored pointer is null. PHP places no
    // limit on simultaneously open wrapper streams, so the table grows by
    // doubling through `__rt_user_wrapper_handles_reserve` rather than exposing a
    // fixed capacity — Gate 1 forbids PHP-visible fixed capacities.
    out.push_str(&comm_directive("_user_wrapper_handles_ptr", 8, target));
    // _user_wrapper_handles_cap: number of allocated handle slots, zero until the
    // table is first reserved.
    out.push_str(&comm_directive("_user_wrapper_handles_cap", 8, target));
    // _user_wrapper_drain_buf: 1 MiB accumulation buffer for the codegen-level
    // feof-gated read loop emitted by stream_get_contents on a wrapper fd.
    // Each fread chunk is copied here, building one contiguous result. Drains
    // larger than 1 MiB are truncated (v1).
    out.push_str(&comm_directive("_user_wrapper_drain_buf", 1048576, target));
    // phar:// write stream state. _phar_write_out is the 1 MiB in-memory
    // payload buffer (template prefix + entry content); _phar_write_len is the
    // bytes used; _phar_write_tpl_len locates the entry payload. The path and
    // entry ptr/len pairs let literal writes call the elephc-phar
    // read-modify-write bridge. The url ptr/len pair keeps a runtime-built
    // phar:// URL alive until fclose() can route it through the URL bridge.
    // Fallback only: one stream at a time; synthetic fd 0x50000000.
    out.push_str(&comm_directive("_phar_write_out", 1048576, target));
    out.push_str(&comm_directive("_phar_write_len", 8, target));
    out.push_str(&comm_directive("_phar_write_tpl_len", 8, target));
    out.push_str(&comm_directive("_phar_write_path_ptr", 8, target));
    out.push_str(&comm_directive("_phar_write_path_len", 8, target));
    out.push_str(&comm_directive("_phar_write_entry_ptr", 8, target));
    out.push_str(&comm_directive("_phar_write_entry_len", 8, target));
    out.push_str(&comm_directive("_phar_write_url_ptr", 8, target));
    out.push_str(&comm_directive("_phar_write_url_len", 8, target));
    // _stream_open_opened_path_scratch: 16-byte scratch backing the 5th
    // `?string &$opened_path` parameter of stream_open. The runtime passes
    // its address so wrappers that follow the PHP-faithful signature can
    // safely write to it; elephc v1 zeroes the slot before each call and
    // does not read the value back.
    out.push_str(&comm_directive("_stream_open_opened_path_scratch", 16, target));
    // _user_filter_registry: 128 (filter_name, class_name) registrations,
    // each entry 32 bytes (filter_name_ptr/len + class_name_ptr/len). Slot
    // is free when filter_name_ptr is null. User filter IDs are slot_index
    // + USER_FILTER_ID_BASE (128) so they don't collide with the existing
    // u8 built-in filter IDs (1..=4). 128 × 32 = 4096 bytes.
    out.push_str(&comm_directive("_user_filter_registry", 4096, target));
    // _user_filter_instances: one wrapper-class instance per attached
    // filter, keyed by (fd, direction). Slot = _user_filter_instances[fd*2
    // + dir] where dir=0 is read, dir=1 is write. Slot is null when no
    // user filter is attached. 256 fds × 2 dirs × 8 B = 4096 bytes.
    out.push_str(&comm_directive("_user_filter_instances", 4096, target));
    // _user_filter_closing: the `$closing` value the next brigade dispatch passes to
    // `filter()`. Every dispatch on the read/write path is a non-closing one; only the
    // flush `stream_filter_remove()` performs sets it, and clears it again straight away.
    out.push_str(&comm_directive("_user_filter_closing", 8, target));
    // _user_filter_last_psfs: the PSFS code the last brigade dispatch returned. The
    // dispatcher itself answers with a buffer/length pair, so the code — which decides
    // whether a closing flush failed — has nowhere else to travel.
    out.push_str(&comm_directive("_user_filter_last_psfs", 8, target));
    // _uwmh_head / _uwmh_tail: the (ptr, len) pairs naming the CALLER and the missing method for
    // the next wrapper path-operation dispatch. One runtime helper serves unlink/rename/mkdir/
    // rmdir and the whole stream_metadata family, so it cannot know which builtin called it; the
    // lowering publishes the pair before dispatching and the helper reads it only on the
    // missing-method path, which runs no user code, so nothing can overwrite it in between.
    out.push_str(&comm_directive("_uwmh_head", 16, target));
    out.push_str(&comm_directive("_uwmh_tail", 16, target));
    // _user_filter_consumed_scratch: the storage `filter()`'s by-reference `&$consumed` binds to.
    // An untyped by-ref parameter is an Int by-ref, so the method writes a plain i64 THROUGH the
    // address it is handed — exactly like `stream_open`'s `&$opened_path` scratch. It used to be
    // handed a Mixed cell instead, whose first word is the tag: the method read 0 (the int tag)
    // as the starting value, which looked right, and then wrote its count OVER the tag.
    out.push_str(&comm_directive("_user_filter_consumed_scratch", 8, target));
    // _user_filter_last_consumed: the value `filter()` left in its by-reference `&$consumed`.
    // php reports it as `fwrite()`'s return on a filtered write stream — a filter that never
    // touches the parameter makes `fwrite()` answer 0 even though the bytes reached the file —
    // and, like the PSFS code, it cannot travel in the buffer/length pair the dispatcher answers
    // with.
    out.push_str(&comm_directive("_user_filter_last_consumed", 8, target));
    // _stream_context_options: transient scratch used while constructing or
    // selecting a registry-backed ContextState for legacy wrapper consumers.
    out.push_str(&comm_directive("_stream_context_options", 8, target));
    // _stream_default_context_handle: process/request-global owner of the
    // lazily allocated default ContextState registry handle.
    out.push_str(&comm_directive("_stream_default_context_handle", 8, target));
    // _stream_current_context_handle: borrowed handle selected by the active
    // stream opener for user-wrapper `$this->context` injection.
    out.push_str(&comm_directive("_stream_current_context_handle", 8, target));
    // _zlib_wrapper_level: the `zlib.level` the active `compress.zlib://` open read
    // out of its stream context, clamped to zlib's -1..9. The deflate initialization
    // is emitted inline with the descriptor already in the result register and no
    // frame of its own, so a run-time level cannot travel in a register or a stack
    // slot; the opener publishes it here and the initialization loads it.
    out.push_str(&comm_directive("_zlib_wrapper_level", 8, target));
    // _last_dir_handle: the opaque registry handle of the directory stream most
    // recently opened by `opendir()` — which `dir()` is built on, so it feeds the
    // slot too. `readdir()`/`rewinddir()`/`closedir()` called with no argument, or
    // with an explicit null, read it instead of an operand. It holds the HANDLE and
    // never a borrowed resource box: the generation stamped into the handle is what
    // makes a stale slot detectable, so no clearing is needed when the stream closes
    // — `__rt_stream_fd` simply stops resolving it and the family raises php's
    // `TypeError: No resource supplied`. `fopen()` deliberately does not write here.
    out.push_str(&comm_directive("_last_dir_handle", 8, target));
    // _http_resp_header_end: byte offset of the body start within
    // _http_resp_buf, set by __rt_http_open after the CRLFCRLF scan.
    out.push_str(&comm_directive("_http_resp_header_end", 8, target));
    // _eir_global_http_u_response_u_header: global slot for $http_response_header.
    // The mangled name (underscore → _u) matches ir_global_symbol("http_response_header").
    out.push_str(&comm_directive("_eir_global_http_u_response_u_header", 8, target));
    // var_dump body literals (rodata): per-element prefix/suffix bytes used by
    // the array/hash walkers. NONE of them carry a leading indent: every
    // var_dump line is padded by `__rt_vd_pad`, which writes `_vd_indent`
    // spaces first, so one set of literals serves every nesting depth.
    out.push_str(".globl _vd_indent_open\n_vd_indent_open:\n    .ascii \"[\"\n");
    out.push_str(".globl _vd_close_arrow\n_vd_close_arrow:\n    .ascii \"]=>\\n\"\n");
    out.push_str(".globl _vd_int_prefix\n_vd_int_prefix:\n    .ascii \"int(\"\n");
    // A RESOURCE nested in a container renders like a top-level one:
    // `resource(4) of type (stream)`. `__rt_var_dump_value` answered NULL for it, so a
    // resource inside ANY array, hash or object printed as null.
    out.push_str(".globl _vd_res_prefix\n_vd_res_prefix:\n    .ascii \"resource(\"\n");
    out.push_str(".globl _vd_res_middle\n_vd_res_middle:\n    .ascii \") of type (\"\n");
    out.push_str(".globl _vd_close_paren\n_vd_close_paren:\n    .ascii \")\\n\"\n");
    out.push_str(".globl _vd_str_prefix\n_vd_str_prefix:\n    .ascii \"string(\"\n");
    out.push_str(".globl _vd_close_paren_space\n_vd_close_paren_space:\n    .ascii \") \\\"\"\n");
    out.push_str(".globl _vd_close_quote\n_vd_close_quote:\n    .ascii \"\\\"\\n\"\n");
    // var_dump bool literals — preformatted lines (11 / 12 bytes) so the bool
    // emitter is a single dispatch + write after the indent pad.
    out.push_str(".globl _vd_bool_true_line\n_vd_bool_true_line:\n    .ascii \"bool(true)\\n\"\n");
    out.push_str(".globl _vd_bool_false_line\n_vd_bool_false_line:\n    .ascii \"bool(false)\\n\"\n");
    out.push_str(".globl _vd_float_prefix\n_vd_float_prefix:\n    .ascii \"float(\"\n");
    out.push_str(".globl _vd_null_line\n_vd_null_line:\n    .ascii \"NULL\\n\"\n");
    // var_dump hash (associative array) string-key delimiters: `["` before the
    // key bytes and `"]=>\n` after, matching PHP's `["key"]=>` line format.
    out.push_str(".globl _vd_str_key_open\n_vd_str_key_open:\n    .ascii \"[\\\"\"\n");
    out.push_str(".globl _vd_str_key_close\n_vd_str_key_close:\n    .ascii \"\\\"]=>\\n\"\n");
    // var_dump nested-container delimiters: `array(` + count + `) {\n` opens a
    // nested array/hash on its value line, `}\n` closes it at the same indent.
    out.push_str(".globl _vd_array_prefix\n_vd_array_prefix:\n    .ascii \"array(\"\n");
    out.push_str(".globl _vd_brace_open\n_vd_brace_open:\n    .ascii \") {\\n\"\n");
    out.push_str(".globl _vd_brace_close\n_vd_brace_close:\n    .ascii \"}\\n\"\n");
    // _vd_indent: current var_dump line indentation, in spaces. The var_dump
    // builtin sets it to 2 around a top-level array body and back to 0 after;
    // `__rt_var_dump_value` bumps it by 2 across each nested container walk.
    out.push_str(&comm_directive("_vd_indent", 8, target));
    // var_dump object delimiters: `object(` + class name + `)#` + the PHP object
    // handle + ` (` + initialized property count + `) {\n` opens an object on its
    // value line; the shared `_vd_brace_close` closes it. The handle is the same
    // small dense integer `spl_object_id()` returns — both read it from
    // `__rt_object_handle_of`, so the printed `#N` and `spl_object_id()` can never
    // disagree. See `runtime::objects::handles` for the pool.
    out.push_str(".globl _vd_object_prefix\n_vd_object_prefix:\n    .ascii \"object(\"\n");
    out.push_str(".globl _vd_object_mid\n_vd_object_mid:\n    .ascii \")#\"\n");
    out.push_str(".globl _vd_object_count_open\n_vd_object_count_open:\n    .ascii \" (\"\n");
    // `uninitialized(` opens the line var_dump prints for a typed property read
    // before its first write; `_vd_close_paren` closes it.
    out.push_str(".globl _vd_uninit_prefix\n_vd_uninit_prefix:\n    .ascii \"uninitialized(\"\n");
    // `*RECURSION*` replaces the value of a container already on the walk stack.
    out.push_str(".globl _vd_recursion_line\n_vd_recursion_line:\n    .ascii \"*RECURSION*\\n\"\n");
    // _vd_seen / _vd_seen_n: the var_dump recursion guard — a bounded stack of
    // the object pointers currently being walked, pushed and popped around each
    // object body by `__rt_var_dump_value`'s tag-6 branch. 256 entries × 8 B.
    // The capacity MUST match `VD_SEEN_CAPACITY` in `runtime::io::var_dump_object`:
    // a lookup that reaches the cap reports recursion, which is what bounds the walk.
    out.push_str(&comm_directive("_vd_seen", 2048, target));
    out.push_str(&comm_directive("_vd_seen_n", 8, target));
    // print_r body literals (rodata): PHP's `Array\n(\n` header, `)\n` footer,
    // `[`/`] => ` key delimiters (unquoted keys, unlike var_dump), a lone
    // newline, the `1` rendered for boolean true, and a 64-space pad used by
    // the recursive indentation helper (written in <=64-byte chunks).
    out.push_str(".globl _pr_array_hdr\n_pr_array_hdr:\n    .ascii \"Array\\n\"\n");
    out.push_str(".globl _pr_open\n_pr_open:\n    .ascii \"(\\n\"\n");
    out.push_str(".globl _pr_close\n_pr_close:\n    .ascii \")\\n\"\n");
    out.push_str(".globl _pr_lbrack\n_pr_lbrack:\n    .ascii \"[\"\n");
    out.push_str(".globl _pr_arrow\n_pr_arrow:\n    .ascii \"] => \"\n");
    out.push_str(".globl _pr_nl\n_pr_nl:\n    .ascii \"\\n\"\n");
    out.push_str(".globl _pr_one\n_pr_one:\n    .ascii \"1\"\n");
    // print_r object literals: the header suffix PHP writes after the class name
    // (`C Object`, and `C Enum` / `C Enum:int` / `C Enum:string` for an enum case),
    // and the ` *RECURSION*` marker a revisited instance renders instead of a body.
    // The marker deliberately carries NO newline — the entry line terminator is
    // written by whichever walker opened the `[key] => ` line, exactly like PHP.
    out.push_str(".globl _pr_object_suffix\n_pr_object_suffix:\n    .ascii \" Object\\n\"\n");
    out.push_str(".globl _pr_enum_suffix\n_pr_enum_suffix:\n    .ascii \" Enum\\n\"\n");
    out.push_str(".globl _pr_enum_int_suffix\n_pr_enum_int_suffix:\n    .ascii \" Enum:int\\n\"\n");
    out.push_str(".globl _pr_enum_str_suffix\n_pr_enum_str_suffix:\n    .ascii \" Enum:string\\n\"\n");
    out.push_str(".globl _pr_recursion\n_pr_recursion:\n    .ascii \" *RECURSION*\"\n");
    // var_dump enum literals: PHP renders an enum case as `enum(Class::Case)`
    // instead of an object body, so the three fragments bracket the class name
    // (from `_class_name_entries`) and the case name (from the instance's `name`
    // property slot).
    out.push_str(".globl _vd_enum_prefix\n_vd_enum_prefix:\n    .ascii \"enum(\"\n");
    out.push_str(".globl _vd_enum_sep\n_vd_enum_sep:\n    .ascii \"::\"\n");
    out.push_str(".globl _vd_enum_close\n_vd_enum_close:\n    .ascii \")\\n\"\n");
    out.push_str(".globl _pr_spaces\n_pr_spaces:\n    .ascii \"                                                                \"\n");
    out.push_str(".globl _ftp_user_cmd\n_ftp_user_cmd:\n    .ascii \"USER anonymous\\x0d\\n\"\n");
    out.push_str(".globl _ftp_pass_cmd\n_ftp_pass_cmd:\n    .ascii \"PASS anonymous@\\x0d\\n\"\n");
    out.push_str(".globl _ftp_type_cmd\n_ftp_type_cmd:\n    .ascii \"TYPE I\\x0d\\n\"\n");
    out.push_str(".globl _ftp_pasv_cmd\n_ftp_pasv_cmd:\n    .ascii \"PASV\\x0d\\n\"\n");
    out.push_str(".globl _ftp_tcp_prefix\n_ftp_tcp_prefix:\n    .ascii \"tcp://\"\n");
    // ftps:// commands (RFC 4217). AUTH TLS upgrades the control connection;
    // PBSZ 0 sets the protection buffer size (always 0 for TLS); PROT P
    // enables private (encrypted) data-channel protection.
    out.push_str(".globl _ftp_auth_tls_cmd\n_ftp_auth_tls_cmd:\n    .ascii \"AUTH TLS\\x0d\\n\"\n");
    out.push_str(".globl _ftp_pbsz_cmd\n_ftp_pbsz_cmd:\n    .ascii \"PBSZ 0\\x0d\\n\"\n");
    out.push_str(".globl _ftp_prot_p_cmd\n_ftp_prot_p_cmd:\n    .ascii \"PROT P\\x0d\\n\"\n");
    out.push_str(&comm_directive("_recvfrom_addr_ptr", 8, target));
    out.push_str(&comm_directive("_recvfrom_addr_len", 8, target));
    out.push_str(&comm_directive("_accept_peer_ptr", 8, target));
    out.push_str(&comm_directive("_accept_peer_len", 8, target));
    // Why the last socket connect/bind failed, published by the socket helpers and read back by
    // the `&$error_code` / `&$error_message` outputs of the four socket-opening builtins.
    out.push_str(&comm_directive("_socket_errno", 8, target));
    // The unresolvable-host message php-src composes instead of reporting an `errno`: the host
    // the resolver was given, the code `getaddrinfo` answered, and the composed text itself. The
    // text lives in a fixed buffer so the caller's `&$error_message` borrows something static,
    // like the `strerror` pointer it borrows for every other failure.
    out.push_str(&comm_directive("_socket_gai_err", 8, target));
    out.push_str(&comm_directive("_socket_gai_host_ptr", 8, target));
    out.push_str(&comm_directive("_socket_gai_host_len", 8, target));
    out.push_str(&comm_directive("_socket_gai_msg_len", 8, target));
    out.push_str(&comm_directive("_socket_gai_msg", SOCKET_GAI_MSG_CAPACITY, target));
    out.push_str(&comm_directive("_filter_missing_msg", crate::codegen_support::runtime::io::FILTER_MISSING_MSG_CAPACITY, target));
    out.push_str(&comm_directive("_str_offset_msg", crate::codegen_support::runtime::objects::STRING_OFFSET_MSG_CAPACITY, target));
    out.push_str(&comm_directive("_open_failed_msg", crate::codegen_support::runtime::io::OPEN_FAILED_MSG_CAPACITY, target));
    out.push_str(&comm_directive("_fopen_bad_mode_reason", crate::codegen_support::runtime::io::BAD_MODE_REASON_CAPACITY, target));
    out.push_str(&comm_directive("_unknown_wrapper_msg", crate::codegen_support::runtime::io::UNKNOWN_WRAPPER_MSG_CAPACITY, target));
    out.push_str(&format!(
        ".globl _gai_msg_prefix\n_gai_msg_prefix:\n    .ascii {GAI_MSG_PREFIX:?}\n"
    ));
    out.push_str(&format!(
        ".globl _gai_msg_middle\n_gai_msg_middle:\n    .ascii {GAI_MSG_MIDDLE:?}\n"
    ));
    out.push_str(&emit_php_wrapper_scheme_table());
    // A `php://filter/...` URL resolved at run time: the filters it names, the direction it asked
    // for, and the resource to open. Published by the parse so the attach can run once that
    // resource is open and boxed; cleared by the attach, so a later plain open cannot inherit
    // them.
    //
    // A LIST, not a single id: `read=string.toupper|string.rot13` names two filters and php runs
    // the bytes through both, in order. The literal path already resolved the whole chain, so a
    // one-slot hand-off was the run-time parse silently answering the first filter's result.
    out.push_str(&comm_directive(
        "_php_filter_pending_ids",
        crate::codegen_support::runtime::io::PHP_FILTER_PENDING_MAX * 8,
        target,
    ));
    out.push_str(&comm_directive("_php_filter_pending_count", 8, target));
    out.push_str(&comm_directive("_php_filter_pending_mode", 8, target));
    out.push_str(&comm_directive("_php_filter_res_ptr", 8, target));
    out.push_str(&comm_directive("_php_filter_res_len", 8, target));
    // The ORIGINAL URL, kept because the swap replaces the caller's filename with the resource
    // and php names the WHOLE URL when the open fails. A non-null pointer is also the only
    // honest "the last parse saw a filter URL" flag: `_php_filter_pending_mode` reads 0 exactly
    // when every name in the chain failed to resolve, which is a filter URL, not a plain path.
    out.push_str(&comm_directive("_php_filter_url_ptr", 8, target));
    out.push_str(&comm_directive("_php_filter_url_len", 8, target));
    // The names that resolved to NO filter, as spans into the URL. php warns twice for each,
    // and the run-time parse used to drop them where the literal parse now reports them.
    out.push_str(&comm_directive(
        "_php_filter_unknown_ptr",
        crate::codegen_support::runtime::io::PHP_FILTER_PENDING_MAX * 8,
        target,
    ));
    out.push_str(&comm_directive(
        "_php_filter_unknown_len",
        crate::codegen_support::runtime::io::PHP_FILTER_PENDING_MAX * 8,
        target,
    ));
    out.push_str(&comm_directive("_php_filter_unknown_count", 8, target));
    // The direction the URL's OWN prefix named: 1 = `read=`, 2 = `write=`, 3 = no prefix.
    // Kept apart from `_php_filter_pending_mode`, which is zeroed when nothing resolved.
    out.push_str(&comm_directive("_php_filter_url_dir", 8, target));
    // The directions the OPEN MODE selects: bit 0 = read, bit 1 = write. php applies a
    // prefix-less filter list once per direction it applies, so `r+` warns twice per unknown
    // name and `x` — a mode naming neither — warns not at all.
    out.push_str(&comm_directive("_php_filter_open_dirs", 8, target));
    // One frame per filtered open IN FLIGHT. A user wrapper's `stream_open` is PHP and may open
    // something itself, and that inner open republishes the single-slot hand-off above; each
    // open therefore saves the URL it must name — and, by saving a null for a non-filter open,
    // whether it opened a suppression scope at all — and reads its OWN frame back on the way
    // out, instead of a global another open has since overwritten.
    for symbol in ["_php_filter_open_url_ptr", "_php_filter_open_url_len"] {
        out.push_str(&comm_directive(
            symbol,
            crate::codegen_support::runtime::io::PHP_FILTER_OPEN_DEPTH_MAX * 8,
            target,
        ));
    }
    out.push_str(&comm_directive("_php_filter_open_depth", 8, target));
    // One PARKED hand-off per open in flight, at a fixed stride so the depth reaches a frame with
    // a shift. Everything above that the parse publishes and something after the OPEN reads lives
    // here for the length of that open: the opener can run PHP — a user wrapper's `stream_open` —
    // and PHP that opens anything re-enters the parse, which republishes every one of those
    // globals. The outer open then attached the inner URL's chain, which its own consumer had
    // already cleared, and answered `abc` where php answers `ABC`.
    out.push_str(&comm_directive(
        "_php_filter_pending_stack",
        crate::codegen_support::runtime::io::PHP_FILTER_OPEN_DEPTH_MAX
            * crate::codegen_support::runtime::io::PHP_FILTER_PENDING_FRAME_SLOTS
            * 8,
        target,
    ));
    out.push_str(&comm_directive("_php_filter_pending_depth", 8, target));
    // The filter machinery's OWN suppression depth, kept apart from `_rt_diag_suppression`.
    // Both silence `__rt_diag_warning`, and `@` still silences everything by raising the other
    // one; what only this counter can do is STAND DOWN for the length of a user wrapper's
    // `stream_open`, which is PHP and whose warnings php prints. Sharing one counter meant the
    // scope a filtered open opens for its inner OPENER also swallowed the wrapper's own PHP.
    out.push_str(&comm_directive("_php_filter_suppression", 8, target));
    // Needles the parse matches, kept as data so one spelling serves both assembly emitters.
    out.push_str(".globl _pf_n_prefix\n_pf_n_prefix:\n    .ascii \"php://filter/\"\n");
    out.push_str(".globl _pf_n_read\n_pf_n_read:\n    .ascii \"read=\"\n");
    out.push_str(".globl _pf_n_write\n_pf_n_write:\n    .ascii \"write=\"\n");
    out.push_str(".globl _pf_n_resource\n_pf_n_resource:\n    .ascii \"/resource=\"\n");
    out.push_str(".globl _data_n_prefix\n_data_n_prefix:\n    .ascii \"data://\"\n");
    out.push_str(".globl _data_n_b64\n_data_n_b64:\n    .ascii \";base64\"\n");
    out.push_str(&comm_directive("_protoent_buf", 32768, target));
    out.push_str(".globl _etc_protocols_path\n_etc_protocols_path:\n    .asciz \"/etc/protocols\"\n");
    out.push_str(&comm_directive("_servent_buf", 1048576, target));
    out.push_str(".globl _etc_services_path\n_etc_services_path:\n    .asciz \"/etc/services\"\n");
    out.push_str(&comm_directive("_principal_lookup_buf", 4096, target));
    out.push_str(".globl _etc_passwd_path\n_etc_passwd_path:\n    .asciz \"/etc/passwd\"\n");
    out.push_str(".globl _etc_group_path\n_etc_group_path:\n    .asciz \"/etc/group\"\n");
    out.push_str(".globl _principal_lookup_read_mode\n_principal_lookup_read_mode:\n    .asciz \"r\"\n");
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
    out.push_str(".globl _resource_type_stream\n_resource_type_stream:\n    .ascii \"stream\"\n");
    // php names a filter resource `stream filter`, not `stream`: `var_dump()` and
    // `get_resource_type()` both read this, and a filter answering `stream` made the two
    // resource kinds indistinguishable from PHP.
    out.push_str(".globl _resource_type_stream_filter\n_resource_type_stream_filter:\n    .ascii \"stream filter\"\n");
    out.push_str(".globl _resource_type_unknown\n_resource_type_unknown:\n    .ascii \"Unknown\"\n");
    out.push_str(".globl _fmt_g\n_fmt_g:\n    .asciz \"%.14G\"\n");
    out.push_str(".globl _fmt_star_e\n_fmt_star_e:\n    .asciz \"%.*e\"\n");
    out.push_str(".globl _fmt_star_f\n_fmt_star_f:\n    .asciz \"%.*f\"\n");
    // PHP's own default `ucwords()` separator set, `" \t\r\n\f\v"`. It is a byte SET rather
    // than a substring, and the backend hands this symbol to `__rt_ucwords` whenever the
    // optional `$separators` argument is omitted, so the default and an explicitly written
    // `" \t\r\n\f\v"` take exactly the same code path.
    out.push_str(
        ".globl _ucwords_default_seps\n_ucwords_default_seps:\n    .byte 32, 9, 13, 10, 12, 11\n",
    );
    out.push_str(".globl _b64_encode_tbl\n_b64_encode_tbl:\n    .ascii \"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/\"\n");
    out.push_str(".globl _b64_decode_tbl\n_b64_decode_tbl:\n");

    // php-src's `base64_reverse_table`, transposed from its signed `short` entries onto
    // unsigned bytes: an alphabet character keeps its 0-63 sextet value, `-1` (skippable
    // whitespace) becomes `B64_DECODE_SKIP`, and `-2` (everything else, including `=`)
    // becomes `B64_DECODE_INVALID`. `__rt_base64_decode` needs the two rejection classes
    // apart: whitespace is dropped in BOTH modes, while any other stray byte is dropped in
    // the lax mode and makes `$strict = true` return `false`. Encoding both as 0 — the old
    // table's behavior — is what silently decoded `"SGVs bG8="` to garbage.
    let mut decode_tbl = vec![B64_DECODE_INVALID; 256];
    for (i, &c) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .iter()
        .enumerate()
    {
        decode_tbl[c as usize] = i as u8;
    }
    for &c in B64_DECODE_WHITESPACE {
        decode_tbl[c as usize] = B64_DECODE_SKIP;
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
    out.push_str(".globl _parse_url_key_scheme\n_parse_url_key_scheme:\n    .ascii \"scheme\"\n");
    out.push_str(".globl _parse_url_key_host\n_parse_url_key_host:\n    .ascii \"host\"\n");
    out.push_str(".globl _parse_url_key_port\n_parse_url_key_port:\n    .ascii \"port\"\n");
    out.push_str(".globl _parse_url_key_user\n_parse_url_key_user:\n    .ascii \"user\"\n");
    out.push_str(".globl _parse_url_key_pass\n_parse_url_key_pass:\n    .ascii \"pass\"\n");
    out.push_str(".globl _parse_url_key_path\n_parse_url_key_path:\n    .ascii \"path\"\n");
    out.push_str(".globl _parse_url_key_query\n_parse_url_key_query:\n    .ascii \"query\"\n");
    out.push_str(".globl _parse_url_key_fragment\n_parse_url_key_fragment:\n    .ascii \"fragment\"\n");
    out.push_str(&format!(
        ".globl _parse_url_component_error_prefix\n_parse_url_component_error_prefix:\n    .ascii {:?}\n",
        "parse_url(): Argument #2 ($component) must be a valid URL component identifier, "
    ));
    out.push_str(".globl _parse_url_component_error_suffix\n_parse_url_component_error_suffix:\n    .ascii \" given\"\n");
    out.push_str(".globl _meta_key_timed_out\n_meta_key_timed_out:\n    .ascii \"timed_out\"\n");
    out.push_str(".globl _meta_key_blocked\n_meta_key_blocked:\n    .ascii \"blocked\"\n");
    out.push_str(".globl _meta_key_eof\n_meta_key_eof:\n    .ascii \"eof\"\n");
    out.push_str(".globl _meta_key_unread_bytes\n_meta_key_unread_bytes:\n    .ascii \"unread_bytes\"\n");
    out.push_str(".globl _meta_key_stream_type\n_meta_key_stream_type:\n    .ascii \"stream_type\"\n");
    out.push_str(".globl _meta_key_wrapper_type\n_meta_key_wrapper_type:\n    .ascii \"wrapper_type\"\n");
    out.push_str(".globl _meta_key_mode\n_meta_key_mode:\n    .ascii \"mode\"\n");
    out.push_str(".globl _meta_key_seekable\n_meta_key_seekable:\n    .ascii \"seekable\"\n");
    out.push_str(".globl _meta_key_uri\n_meta_key_uri:\n    .ascii \"uri\"\n");
    // The response header lines an `http://` stream carries, the same array php publishes as
    // `$http_response_header`.
    out.push_str(".globl _meta_key_wrapper_data\n_meta_key_wrapper_data:\n    .ascii \"wrapper_data\"\n");
    // A `data:` URI contributes its own metadata keys: the media type, every `name=value`
    // parameter under its own name, and `base64` — which php emits even when it is false.
    out.push_str(".globl _meta_key_mediatype\n_meta_key_mediatype:\n    .ascii \"mediatype\"\n");
    out.push_str(".globl _meta_key_base64\n_meta_key_base64:\n    .ascii \"base64\"\n");
    out.push_str(".globl _meta_stype_stdio\n_meta_stype_stdio:\n    .ascii \"STDIO\"\n");
    out.push_str(".globl _meta_stype_socket\n_meta_stype_socket:\n    .ascii \"tcp_socket\"\n");
    // The names php-src gives the stream kinds elephc can produce. They are wrapper and
    // backend identities, not descriptor properties: `php://memory` reports MEMORY whether or
    // not it is seekable, and a pipe from `popen()` reports STDIO although it is not.
    out.push_str(".globl _meta_stype_memory\n_meta_stype_memory:\n    .ascii \"MEMORY\"\n");
    out.push_str(".globl _meta_stype_temp\n_meta_stype_temp:\n    .ascii \"TEMP\"\n");
    out.push_str(".globl _meta_stype_output\n_meta_stype_output:\n    .ascii \"Output\"\n");
    out.push_str(".globl _meta_stype_input\n_meta_stype_input:\n    .ascii \"Input\"\n");
    out.push_str(".globl _meta_stype_dir\n_meta_stype_dir:\n    .ascii \"dir\"\n");
    out.push_str(".globl _meta_stype_zip\n_meta_stype_zip:\n    .ascii \"zip\"\n");
    out.push_str(".globl _meta_stype_glob\n_meta_stype_glob:\n    .ascii \"glob\"\n");
    // php-src names a TCP socket after the ssl-capable transport whenever the ssl transport
    // exists in the build, which for elephc is whenever a program can ask for TLS at all.
    out.push_str(".globl _meta_stype_tcp\n_meta_stype_tcp:\n    .ascii \"tcp_socket/ssl\"\n");
    out.push_str(".globl _meta_stype_udp\n_meta_stype_udp:\n    .ascii \"udp_socket\"\n");
    out.push_str(".globl _meta_stype_unix\n_meta_stype_unix:\n    .ascii \"unix_socket\"\n");
    out.push_str(".globl _meta_stype_generic\n_meta_stype_generic:\n    .ascii \"generic_socket\"\n");
    out.push_str(".globl _meta_wrapper_plainfile\n_meta_wrapper_plainfile:\n    .ascii \"plainfile\"\n");
    out.push_str(".globl _meta_wrapper_http\n_meta_wrapper_http:\n    .ascii \"http\"\n");
    out.push_str(".globl _meta_wrapper_https\n_meta_wrapper_https:\n    .ascii \"https\"\n");
    out.push_str(".globl _meta_wrapper_ftp\n_meta_wrapper_ftp:\n    .ascii \"ftp\"\n");
    out.push_str(".globl _meta_wrapper_ftps\n_meta_wrapper_ftps:\n    .ascii \"ftps\"\n");
    out.push_str(".globl _meta_wrapper_phar\n_meta_wrapper_phar:\n    .ascii \"phar\"\n");
    out.push_str(".globl _meta_wrapper_php\n_meta_wrapper_php:\n    .ascii \"PHP\"\n");
    out.push_str(".globl _meta_wrapper_data\n_meta_wrapper_data:\n    .ascii \"RFC2397\"\n");
    out.push_str(".globl _meta_wrapper_zlib\n_meta_wrapper_zlib:\n    .ascii \"compress.zlib\"\n");
    out.push_str(".globl _meta_wrapper_bzip2\n_meta_wrapper_bzip2:\n    .ascii \"compress.bzip2\"\n");
    out.push_str(".globl _meta_wrapper_glob\n_meta_wrapper_glob:\n    .ascii \"glob\"\n");
    out.push_str(".globl _meta_wrapper_user\n_meta_wrapper_user:\n    .ascii \"user-space\"\n");
    // php's zip wrapper names ITSELF `zip wrapper` in `wrapper_type`, unlike every other
    // built-in, whose name has no such suffix. Measured on `php -n` 8.5.6.
    out.push_str(".globl _meta_wrapper_zip\n_meta_wrapper_zip:\n    .ascii \"zip wrapper\"\n");
    out.push_str(".globl _meta_mode_r\n_meta_mode_r:\n    .ascii \"r\"\n");
    out.push_str(".globl _meta_mode_w\n_meta_mode_w:\n    .ascii \"w\"\n");
    out.push_str(".globl _meta_mode_rw\n_meta_mode_rw:\n    .ascii \"r+\"\n");
    out.push_str(".p2align 3\n");
    out.push_str(".globl _tmpfile_template\n_tmpfile_template:\n    .ascii \"/tmp/elephc-XXXXXX\\0\"\n    .byte 0,0,0,0,0\n");
    // The name `mkstemp` resolved for the last `tmpfile()`. PHP reports the file it created
    // as the stream URI, and the template above is consumed on the helper's stack, so the
    // resolved name has to be published before the helper unlinks and returns.
    out.push_str(&comm_directive("_tmpfile_last_path", 32, target));
    out.push_str(".globl _locale_utf8_name\n_locale_utf8_name:\n    .asciz \"C.UTF-8\"\n");
    out.push_str(".globl _locale_env_name\n_locale_env_name:\n    .asciz \"\"\n");
    out.push_str(&system::emit_json_data());
    out.push_str(&system::emit_date_data());
    out.push_str(&system::emit_strtotime_data());
    out.push_str(&emit_php_uname_data());

    out
}

/// Emit symbol data for all first-class-callable builtin functions.
///
/// Produces per-name labels (`_callable_builtin_name_N`), a null-terminated
/// `"__invoke"` string for `__invoke` lookups, `_callable_builtin_count`
/// holding the total count, and `_callable_builtin_table` containing
/// pointer/length pairs for each builtin. Used by the `is_callable()` runtime
/// routine and callable-invoke paths.
fn emit_builtin_callable_data(target: Target) -> String {
    let mut out = String::new();
    let strict_builtins = supported_builtin_function_names_for_profile(true);
    let mut builtins = all_supported_builtin_function_names();
    builtins.sort_by_key(|name| !strict_builtins.contains(name));
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
    out.push_str(
        ".globl _callable_builtin_strict_count\n_callable_builtin_strict_count:\n",
    );
    out.push_str(&format!("    .quad {}\n", strict_builtins.len()));
    out.push_str(&comm_directive("_callable_strict_profile", 8, target));
    out.push_str(".globl _callable_builtin_table\n_callable_builtin_table:\n");
    for (idx, name) in builtins.iter().enumerate() {
        out.push_str(&format!("    .quad _callable_builtin_name_{}\n", idx));
        out.push_str(&format!("    .quad {}\n", name.len()));
    }
    out
}

/// Emit the `php_uname_mode_len_msg` and `php_uname_mode_value_msg`
/// error message strings used when `php_uname()` mode argument validation fails.
fn emit_php_uname_data() -> String {
    format!(
        ".globl _php_uname_mode_len_msg\n_php_uname_mode_len_msg:\n    .ascii {:?}\n\
         .globl _php_uname_mode_value_msg\n_php_uname_mode_value_msg:\n    .ascii {:?}\n",
        PHP_UNAME_MODE_LEN_MSG, PHP_UNAME_MODE_VALUE_MSG
    )
}

/// Emits the `php://` sub-scheme table `__rt_php_wrapper_open` walks.
///
/// `fopen()` resolves a wrapper scheme from a literal path at compile time. A path built at run
/// time has to make the same choices with the bytes in hand, and a table keeps the names and
/// their actions declared once here rather than spelled out as a chain of inline comparisons in
/// two hand-written assembly emitters.
///
/// Records are a fixed 16 bytes so the walk is a simple stride: name padded to 8 bytes, its
/// length, the action, and whether the name matches as a prefix (`temp/maxmemory:N` and
/// `fd/N` carry a suffix) or must match exactly. A zero length terminates the table.
fn emit_php_wrapper_scheme_table() -> String {
    // (name, action, prefix-match). Actions: 0/1/2 duplicate that descriptor, 3 opens an
    // anonymous temporary buffer, 4 parses the descriptor number out of the rest of the name.
    const SCHEMES: &[(&str, u8, bool)] = &[
        ("stdin", 0, false),
        ("input", 0, false),
        ("stdout", 1, false),
        ("output", 1, false),
        ("stderr", 2, false),
        ("memory", 3, false),
        ("temp", 3, false),
        ("temp/", 3, true),
        ("fd/", 4, true),
    ];
    let mut out = String::new();
    out.push_str(".p2align 3\n");
    out.push_str(".globl _php_wrapper_schemes\n_php_wrapper_schemes:\n");
    for (name, action, prefix) in SCHEMES {
        assert!(name.len() <= 8, "php:// sub-scheme name must fit the 8-byte field");
        let padded: String = name.chars().chain(std::iter::repeat('\0')).take(8).collect();
        out.push_str(&format!("    .ascii {:?}\n", padded));
        out.push_str(&format!("    .byte {}\n", name.len()));
        out.push_str(&format!("    .byte {}\n", action));
        out.push_str(&format!("    .byte {}\n", u8::from(*prefix)));
        out.push_str("    .byte 0, 0, 0, 0, 0\n");
    }
    // Terminator: a zero name length ends the walk.
    out.push_str("    .byte 0, 0, 0, 0, 0, 0, 0, 0\n");
    out.push_str("    .byte 0, 0, 0, 0, 0, 0, 0, 0\n");
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Platform};

    /// Every common symbol the runtime data section declares must carry an alignment operand
    /// the target's own assembler reads as 8 bytes.
    ///
    /// This is a whole-section sweep rather than a spot check because the failure is silent at
    /// assembly time and only shows up at link time, once per program rather than once per
    /// symbol: `.comm sym, N, 3` means 8-byte alignment to Mach-O's assembler and 3-byte
    /// alignment to GNU as. An ELF build of an under-aligned symbol assembles fine and then
    /// fails to link with `relocation truncated to fit: R_AARCH64_LDST64_ABS_LO12_NC`, because
    /// that relocation encodes its displacement pre-shifted by 3 and cannot name an address
    /// that is not 8-byte aligned. `_stack_limit` took out every linux-aarch64 link this way.
    #[test]
    fn test_runtime_common_symbols_are_aligned_for_each_object_format() {
        for (platform, arch, expected) in [
            (Platform::MacOS, Arch::AArch64, "3"),
            (Platform::Linux, Arch::AArch64, "8"),
            (Platform::Linux, Arch::X86_64, "8"),
        ] {
            let target = Target { platform, arch };
            let asm = emit_runtime_data_fixed(8_388_608, target);

            let mut seen = 0usize;
            for line in asm.lines().filter(|line| line.starts_with(".comm ")) {
                seen += 1;
                let alignment = line.rsplit(',').next().unwrap().trim();
                assert_eq!(
                    alignment, expected,
                    "{:?}/{:?} emitted `{}`, whose alignment operand is not the {}-spelling \
                     that this object format's assembler reads as 8 bytes",
                    platform, arch, line, expected
                );
            }
            assert!(
                seen > 100,
                "expected the fixed runtime data to declare its usual common symbols, saw {}",
                seen
            );
        }
    }

    /// Pins the symbol whose under-alignment broke linux-aarch64 linking, so a future
    /// hand-written `.comm` for it cannot regress past the sweep above.
    #[test]
    fn test_stack_limit_is_eight_byte_aligned_on_elf() {
        let asm = emit_runtime_data_fixed(
            8_388_608,
            Target {
                platform: Platform::Linux,
                arch: Arch::AArch64,
            },
        );

        assert!(asm.contains(".comm _stack_limit, 8, 8\n"));
        assert!(asm.contains(".comm _stack_limit_main, 8, 8\n"));
    }
}
