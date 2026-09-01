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
    HASH_UNKNOWN_ALGO_MSG, HASH_UPDATE_FINALIZED_CTX_MSG, ICONV_STRPOS_OFFSET_MSG,
    MB_STRLEN_UNKNOWN_ENCODING_MSG,
    OB_CLOSURE_INVOKE_NAME, OB_DEFAULT_HANDLER_NAME, OB_FATAL_IN_HANDLER, OB_NTC_CREATE_FAIL,
    OB_NTC_G_CLEAN, OB_NTC_G_END_CLEAN, OB_NTC_G_END_FLUSH, OB_NTC_G_FLUSH, OB_NTC_G_GET_CLEAN,
    OB_NTC_G_GET_FLUSH, OB_NTC_NO_CLEAN, OB_NTC_NO_END_CLEAN, OB_NTC_NO_END_FLUSH,
    OB_NTC_NO_FLUSH, OB_NTC_NO_GET_FLUSH, OBJECT_NOT_ARRAY_PREFIX, OBJECT_NOT_ARRAY_SUFFIX,
    OB_WARN_BAD_CALLBACK_GENERIC,
    OB_WARN_BAD_CALLBACK_PREFIX, OB_WARN_BAD_CALLBACK_SUFFIX,
    PHP_UNAME_MODE_LEN_MSG, PHP_UNAME_MODE_VALUE_MSG, SPRINTF_ARGCOUNT_MSG,
    SPRINTF_ARRAY_TO_STRING_WARNING, SPRINTF_OBJECT_NUMERIC_WARNING_PREFIX,
    SPRINTF_OBJECT_TO_FLOAT_WARNING_SUFFIX, SPRINTF_OBJECT_TO_INT_WARNING_SUFFIX,
    SPRINTF_OVERFLOW_MSG, SPRINTF_UNKNOWN_SPEC_MSG, SPRINTF_WIDTH_MSG, STACK_OVERFLOW_MSG,
    STR_REPEAT_TIMES_MSG, UNSER_ALLOWED_CLASSES_ENTRY_PREFIX,
    UNSER_ALLOWED_CLASSES_POLICY_PREFIX, UNSER_OBJECT_STRING_ERROR_PREFIX,
    UNSER_OBJECT_STRING_ERROR_SUFFIX, UNSER_OPTIONS_TYPE_PREFIX, UNSER_TYPE_GIVEN_SUFFIX,
    UNCAUGHT_DATEPERIOD_STACK_PREFIX, UNCAUGHT_DATETIME_FORMAT_PARENT_PREFIX,
    UNCAUGHT_DATETIME_FORMAT_STACK_PREFIX, UNCAUGHT_DATETIME_FORMAT_STACK_SUFFIX,
    UNCAUGHT_TIMEZONE_OFFSET_STACK_PREFIX, UNCAUGHT_TIMEZONE_OFFSET_STACK_SUFFIX,
    UNCAUGHT_TRACE_CLASS_SEPARATOR, UNCAUGHT_TRACE_LINE_PREFIX, UNCAUGHT_TRACE_LOCATION_SEPARATOR,
    UNCAUGHT_TRACE_NEWLINE, UNCAUGHT_TRACE_NEXT_PREFIX, UNCAUGHT_TRACE_NEXT_STACK_PREFIX,
    UNCAUGHT_TRACE_PREFIX,
    UNCAUGHT_UNSERIALIZE_CALL_AFTER_LINE, UNCAUGHT_UNSERIALIZE_CALL_PREFIX,
    UNCAUGHT_UNSERIALIZE_CALL_SUFFIX, UNCAUGHT_UNSERIALIZE_OWNER_SUFFIX,
    UNCAUGHT_UNSERIALIZE_STACK_PREFIX, UNCAUGHT_UNSERIALIZE_THROWN_SUFFIX,
};
use super::super::system;
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
/// `float_precision` is PHP's compile-time `precision` INI value for ordinary
/// float-to-string conversion and is part of the runtime cache identity.
/// `php_profile` is the selected PHP minor version and keeps version-sensitive
/// constants in the runtime data aligned with the user-code constant surface.
pub(crate) fn emit_runtime_data_fixed(
    heap_size: usize,
    target: Target,
    float_precision: u8,
    php_profile: u8,
) -> String {
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
    out.push_str(".globl _sprintf_closure_class_name\n_sprintf_closure_class_name:\n    .ascii \"Closure\"\n");
    out.push_str(&format!(
        ".globl _diag_sprintf_array_to_string\n_diag_sprintf_array_to_string:\n    .ascii {SPRINTF_ARRAY_TO_STRING_WARNING:?}\n"
    ));
    out.push_str(&format!(
        ".globl _diag_sprintf_object_numeric_prefix\n_diag_sprintf_object_numeric_prefix:\n    .ascii {SPRINTF_OBJECT_NUMERIC_WARNING_PREFIX:?}\n"
    ));
    out.push_str(&format!(
        ".globl _diag_sprintf_object_to_int_suffix\n_diag_sprintf_object_to_int_suffix:\n    .ascii {SPRINTF_OBJECT_TO_INT_WARNING_SUFFIX:?}\n"
    ));
    out.push_str(&format!(
        ".globl _diag_sprintf_object_to_float_suffix\n_diag_sprintf_object_to_float_suffix:\n    .ascii {SPRINTF_OBJECT_TO_FLOAT_WARNING_SUFFIX:?}\n"
    ));
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
    out.push_str(&comm_directive("_unser_warning_callback", 8, target));
    out.push_str(&comm_directive("_unser_dateinterval_dynamic_callback", 8, target));
    out.push_str(&comm_directive("_unser_warning_emitted", 8, target));
    out.push_str(&comm_directive("_unser_force_failure", 8, target));
    out.push_str(&comm_directive("_unser_failure_offset", 8, target));
    // Native ext/date __unserialize hooks publish enough call-site state for
    // the uncaught-exception helper to reproduce php-src's internal-hook trace.
    out.push_str(&comm_directive("_unser_trace_active", 8, target));
    out.push_str(&comm_directive("_unser_trace_exception_ptr", 8, target));
    out.push_str(&comm_directive("_unser_trace_input_ptr", 8, target));
    out.push_str(&comm_directive("_unser_trace_input_len", 8, target));
    out.push_str(&comm_directive("_unser_trace_owner_ptr", 8, target));
    out.push_str(&comm_directive("_unser_trace_owner_len", 8, target));
    out.push_str(&comm_directive("_unser_trace_call_line", 8, target));
    out.push_str(&comm_directive("_date_special_trace_kind", 8, target));
    out.push_str(&comm_directive("_date_special_trace_line", 8, target));
    out.push_str(&comm_directive("_date_constructor_trace_line", 8, target));
    out.push_str(&comm_directive("_date_special_trace_exception_ptr", 8, target));
    out.push_str(&comm_directive("_dateperiod_foreach_trace_active", 8, target));
    out.push_str(&comm_directive("_dateperiod_foreach_trace_exception_ptr", 8, target));
    out.push_str(&comm_directive("_dateperiod_foreach_trace_line", 8, target));
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
    // elephc_probe_route_fn: a function-pointer slot the sampling probe fills at init
    // (with elephc_probe_set_route) and the --web bridge reads to tag samples by route.
    // Zero unless --probe linked the probe, so route tagging is pay-for-use with no
    // compile coupling and no dlsym (the symbol lives in the always-linked core runtime).
    out.push_str(&comm_directive(&target.extern_symbol("elephc_probe_route_fn"), 8, target));
    // elephc_probe_rearm_fn: filled with elephc_probe_rearm under --probe; the
    // --web bridge calls it to re-arm the profiling timer in each worker, which
    // the post-fork disarm (that protects exec'd children) turned off.
    out.push_str(&comm_directive(&target.extern_symbol("elephc_probe_rearm_fn"), 8, target));
    // elephc_probe_verify_fn: verifies a signed X-Elephc-Query header against the
    // embedded build key, so turning profiling on stays a privileged act.
    out.push_str(&comm_directive(&target.extern_symbol("elephc_probe_verify_fn"), 8, target));
    // elephc_instr_throw_fn: filled with elephc_instr_throw under monitoring; the
    // single throw helper calls it so the profiler learns that an exception
    // unwound, and when. Without it the frames an exception passed through could
    // only be closed when the CATCHER exits, which charged the handler's work to
    // whatever threw. Null in a binary without the capability, where the helper
    // pays one load and a branch on a path taken only by throws.
    out.push_str(&comm_directive(&target.extern_symbol("elephc_instr_throw_fn"), 8, target));
    // elephc_instr_unpark_fn: filled with elephc_instr_unpark under monitoring.
    // The suspend helper calls it on the three paths that leave without returning
    // to the suspension site — `Fiber::suspend()` outside a fiber, a live
    // `unserialize()`, and a pending `Fiber::throw()` delivered on resume — so the
    // activation is back on the profiler's stack before the handler runs. Null in
    // a binary without the capability, where those paths pay one load and a
    // branch, and only when they are already raising.
    out.push_str(&comm_directive(&target.extern_symbol("elephc_instr_unpark_fn"), 8, target));
    // elephc_monitor_active: 1 once this process has been asked to profile —
    // written by the probe's init, read by the exact profiler's, which runs after
    // it. One check, in one place: repeating it would consume the control
    // channel's marker twice and the second reader would see nothing.
    out.push_str(&comm_directive(&target.extern_symbol("elephc_monitor_active"), 8, target));
    // elephc_probe_allocs_ptr: the ADDRESS of `_gc_allocs`, published under
    // --probe so the sampler can read the allocation counter without declaring
    // that symbol itself. `_gc_allocs` is spelled with a hardcoded underscore
    // everywhere it is emitted, which is self-consistent while only assembly
    // names it; a Rust crate resolving it directly would break every ELF link.
    // Handing over a pointer keeps that name inside the assembly.
    out.push_str(&comm_directive(&target.extern_symbol("elephc_probe_allocs_ptr"), 8, target));
    // elephc_instr_io_fn: a function-pointer slot filled with elephc_instr_io
    // under --instrument, else zero. I/O builtins (PDO queries) read it and call
    // through it when non-null, so the exact profiler can count queries per
    // function — pay-for-use, no dlsym, no coupling to the instrument crate.
    out.push_str(&comm_directive(&target.extern_symbol("elephc_instr_io_fn"), 8, target));
    // elephc_instr_query_fn: companion slot filled with elephc_instr_query under
    // --instrument, else zero. The PDO bridge reads it and reports each query's
    // SQL text (normalized) so the exact profiler can list distinct statements
    // and their execution counts — the N+1 view. Pay-for-use, like the io slot.
    out.push_str(&comm_directive(&target.extern_symbol("elephc_instr_query_fn"), 8, target));
    // elephc_instr_wait_fn: third companion slot, filled with elephc_instr_wait
    // under --instrument. The PDO bridge times the actual driver call and
    // reports the nanoseconds through it, which separates recorded DB wait from
    // each function's remaining wall time. Zero (inert) in a normal binary.
    out.push_str(&comm_directive(&target.extern_symbol("elephc_instr_wait_fn"), 8, target));
    // elephc_instr_trace_fn: fourth companion slot, filled with
    // elephc_instr_trace_begin under --instrument. The web bridge calls it at
    // the start of every request with the inbound W3C `traceparent`, so a
    // profile slice carries the identity of the distributed trace it belongs
    // to. Zero (inert) in a normal binary.
    out.push_str(&comm_directive(&target.extern_symbol("elephc_instr_trace_fn"), 8, target));
    // elephc_instr_request_fn: brackets one request's profile under
    // --web --instrument, so a dormant production binary can profile a single
    // request on demand instead of every request or none.
    out.push_str(&comm_directive(&target.extern_symbol("elephc_instr_request_fn"), 8, target));
    out.push_str(&comm_directive("_rt_diag_suppression", 8, target));
    // PHP 8.4 removed E_STRICT from E_ALL; older profiles retain its bit.
    let default_error_reporting = if php_profile >= 4 { 30719 } else { 32767 };
    out.push_str(".globl _rt_error_reporting\n");
    out.push_str("_rt_error_reporting:\n");
    out.push_str(&format!("    .quad {default_error_reporting}\n"));
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
    // `_resource_id_next` is the never-used PHP RESOURCE id cursor. It starts at 5,
    // not at 1, and that number is measured rather than chosen: under PHP 8.5.6 CLI
    // the three standard streams occupy ids 1..3 (`get_resource_id(STDIN|STDOUT|STDERR)`
    // returns 1, 2, 3), id 4 is consumed by the SAPI before user code runs, and the
    // first resource a script opens is therefore id 5 — identically for `php file.php`
    // and for `php -r`. elephc already renders STDIN/STDOUT/STDERR as 1/2/3, so
    // starting user resources at 5 reproduces reference numbering end to end.
    out.push_str(".globl _resource_id_next\n_resource_id_next:\n    .quad 5\n");
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
        ".globl _iconv_strpos_offset_msg\n_iconv_strpos_offset_msg:\n    .ascii {:?}\n",
        ICONV_STRPOS_OFFSET_MSG
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
    for (label, message) in [
        ("_uncaught_trace_prefix", UNCAUGHT_TRACE_PREFIX),
        ("_uncaught_trace_next_prefix", UNCAUGHT_TRACE_NEXT_PREFIX),
        (
            "_uncaught_trace_next_stack_prefix",
            UNCAUGHT_TRACE_NEXT_STACK_PREFIX,
        ),
        ("_uncaught_trace_class_separator", UNCAUGHT_TRACE_CLASS_SEPARATOR),
        ("_uncaught_trace_location_separator", UNCAUGHT_TRACE_LOCATION_SEPARATOR),
        ("_uncaught_trace_line_prefix", UNCAUGHT_TRACE_LINE_PREFIX),
        ("_uncaught_trace_newline", UNCAUGHT_TRACE_NEWLINE),
        ("_uncaught_dateperiod_stack_prefix", UNCAUGHT_DATEPERIOD_STACK_PREFIX),
        ("_uncaught_timezone_offset_stack_prefix", UNCAUGHT_TIMEZONE_OFFSET_STACK_PREFIX),
        ("_uncaught_timezone_offset_stack_suffix", UNCAUGHT_TIMEZONE_OFFSET_STACK_SUFFIX),
        ("_uncaught_datetime_format_stack_prefix", UNCAUGHT_DATETIME_FORMAT_STACK_PREFIX),
        ("_uncaught_datetime_format_parent_prefix", UNCAUGHT_DATETIME_FORMAT_PARENT_PREFIX),
        ("_uncaught_datetime_format_stack_suffix", UNCAUGHT_DATETIME_FORMAT_STACK_SUFFIX),
        ("_uncaught_unserialize_stack_prefix", UNCAUGHT_UNSERIALIZE_STACK_PREFIX),
        ("_uncaught_unserialize_owner_suffix", UNCAUGHT_UNSERIALIZE_OWNER_SUFFIX),
        ("_uncaught_unserialize_call_prefix", UNCAUGHT_UNSERIALIZE_CALL_PREFIX),
        ("_uncaught_unserialize_call_after_line", UNCAUGHT_UNSERIALIZE_CALL_AFTER_LINE),
        ("_uncaught_unserialize_call_suffix", UNCAUGHT_UNSERIALIZE_CALL_SUFFIX),
        ("_uncaught_unserialize_thrown_suffix", UNCAUGHT_UNSERIALIZE_THROWN_SUFFIX),
    ] {
        out.push_str(&format!(".globl {label}\n{label}:\n    .ascii {message:?}\n"));
    }
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
    out.push_str(".globl _diag_file_get_contents_failed_msg\n_diag_file_get_contents_failed_msg:\n    .ascii \"Warning: file_get_contents(): Failed to open stream\\n\"\n");
    out.push_str(".globl _diag_fopen_failed_msg\n_diag_fopen_failed_msg:\n    .ascii \"Warning: fopen(): Failed to open stream\\n\"\n");
    out.push_str(".globl _swr_bad_proto_msg\n_swr_bad_proto_msg:\n    .ascii \"Warning: stream_wrapper_register(): Invalid protocol scheme specified.\\n\"\n");
    out.push_str(".globl _swr_dup_proto_msg\n_swr_dup_proto_msg:\n    .ascii \"Warning: stream_wrapper_register(): Protocol is already defined.\\n\"\n");
    // -- php-src's unreachable-seek warning fragments, shared with `__rt_file_get_contents_range` --
    // The helper derives its `__rt_concat` length immediates from the same table, so the bytes
    // here and the immediates there can never drift apart.
    for (label, message) in
        crate::codegen_support::runtime::io::FILE_GET_CONTENTS_SEEK_MESSAGES
    {
        out.push_str(&format!(".globl {label}\n{label}:\n    .ascii {message:?}\n"));
    }
    out.push_str(".globl _diag_define_already_defined_msg\n_diag_define_already_defined_msg:\n    .ascii \"Warning: define(): Constant already defined\\n\"\n");
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
    out.push_str(&comm_directive("_eof_flags", 256, target));
    out.push_str(&comm_directive("_popen_files", 2048, target));
    out.push_str(&comm_directive("_dir_handles", 2048, target));
    // Per-fd glob:// state pointers (256 fds × 8B). Each slot is a pointer to
    // a heap-allocated glob_state struct (pathv ptr + pathc + index + the
    // libc glob_t whose lifetime globfree() needs at closedir time). The
    // readdir/closedir/rewinddir helpers probe this table first; a non-zero
    // entry routes them through the glob iterator instead of the libc DIR*.
    out.push_str(&comm_directive("_glob_handles", 2048, target));
    out.push_str(&comm_directive("_stream_read_filters", 256, target));
    out.push_str(&comm_directive("_stream_write_filters", 256, target));
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
    // _elephc_tls_attach_fd_fn: indirect pointer to elephc_tls_attach_fd,
    // used by stream_socket_enable_crypto to promote an existing TCP fd to
    // a TLS session without re-establishing the TCP connection. Same
    // late-binding pattern as the other tls fn slots so non-TLS programs
    // do not pull in elephc-tls at link time.
    out.push_str(&comm_directive("_elephc_tls_attach_fd_fn", 8, target));
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
    // _elephc_iconv_call_fn / _elephc_iconv_release_fn: indirect pointers to the iconv
    // bridge, published only at an iconv*() call site so the shared runtime never names
    // elephc-iconv. Programs that never call one leave the slots null and do not pull in
    // -lelephc_iconv.
    out.push_str(&comm_directive("_elephc_iconv_call_fn", 8, target));
    out.push_str(&comm_directive("_elephc_iconv_release_fn", 8, target));
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
    // _tls_sessions: per-fd TLS handle (i64 returned by
    // elephc_tls_attach_fd or 0 when the fd is plain TCP). Indexed by raw
    // fd up to 256; the runtime fread/fwrite/fclose paths consult this
    // table and route through the elephc-tls helpers when an entry is
    // non-zero, falling back to read/write/close syscalls otherwise.
    out.push_str(&comm_directive("_tls_sessions", 2048, target));
    // _stream_chunk_size: per-fd read/write chunk size set by
    // stream_set_chunk_size, indexed by raw fd up to 256 (8 bytes each). A zero
    // entry means "unset" and reports PHP's default of 8192. stream_set_chunk_size
    // returns the previous value (the PHP-observable contract); the size does not
    // currently change read granularity (reads return identical data).
    out.push_str(&comm_directive("_stream_chunk_size", 2048, target));
    // _stream_connect_host: per-fd transport host string (ptr, len) captured by
    // stream_socket_client so stream_socket_enable_crypto can default the TLS
    // SNI / peer-name to the connection host when no ssl.peer_name context
    // option is set. 256 fds * 16 bytes (ptr + len). A zero len means "unset".
    out.push_str(&comm_directive("_stream_connect_host", 4096, target));
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
    // Key literals used by __rt_get_ssl_peer_name for the
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
    // (_ssl_peer_name_key_str is already defined above for stream_context_get_ssl_peer_name)
    out.push_str(
        ".globl _ssl_allow_self_signed_key_str\n_ssl_allow_self_signed_key_str:\n    .ascii \"allow_self_signed\"\n",
    );
    out.push_str(
        ".globl _ssl_verify_peer_name_key_str\n_ssl_verify_peer_name_key_str:\n    .ascii \"verify_peer_name\"\n",
    );
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
    out.push_str(&comm_directive("_http_active_ignore_errors", 8, target));
    out.push_str(&comm_directive("_http_active_max_redirects", 8, target));
    out.push_str(&comm_directive("_http_active_timeout_seconds", 8, target));
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
    // _user_wrappers: USER_WRAPPER_REGISTRATIONS_CAP = 64 scheme→class
    // registrations, each entry 32 bytes (protocol_ptr/len + class_ptr/len).
    // Slot is free when protocol_ptr is null. 64 × 32 = 2048 bytes.
    out.push_str(&comm_directive("_user_wrappers", 2048, target));
    // _user_wrapper_handles: USER_WRAPPER_HANDLES_CAP = 256 active stream-handle
    // slots, each storing the wrapper object pointer keyed by synthetic fd
    // `USER_WRAPPER_FD_BASE + slot_index`. Slot is free when the stored pointer
    // is null. 256 slots × 8 bytes = 2048 bytes.
    out.push_str(&comm_directive("_user_wrapper_handles", 2048, target));
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
    // _stream_context_options: pointer to the current stream-context
    // options hash (nested array of `wrapper => option => value`).
    // stream_context_create() stores its options arg here; consumers
    // (http://, ftp://, fopen 4th arg) read it back through
    // __rt_hash_get. v1 limitation: only one active context at a time —
    // a fresh stream_context_create overwrites the slot.
    out.push_str(&comm_directive("_stream_context_options", 8, target));
    // var_dump body literals (rodata): per-element prefix/suffix bytes used by
    // the array/hash walkers. NONE of them carry a leading indent: every
    // var_dump line is padded by `__rt_vd_pad`, which writes `_vd_indent`
    // spaces first, so one set of literals serves every nesting depth.
    out.push_str(".globl _vd_indent_open\n_vd_indent_open:\n    .ascii \"[\"\n");
    out.push_str(".globl _vd_close_arrow\n_vd_close_arrow:\n    .ascii \"]=>\\n\"\n");
    out.push_str(".globl _vd_int_prefix\n_vd_int_prefix:\n    .ascii \"int(\"\n");
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
    out.push_str(".globl _resource_type_unknown\n_resource_type_unknown:\n    .ascii \"Unknown\"\n");
    out.push_str(&format!(
        ".globl _fmt_g\n_fmt_g:\n    .asciz \"%.{}G\"\n",
        float_precision
    ));
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
    out.push_str(".globl _meta_stype_stdio\n_meta_stype_stdio:\n    .ascii \"STDIO\"\n");
    out.push_str(".globl _meta_stype_socket\n_meta_stype_socket:\n    .ascii \"tcp_socket\"\n");
    out.push_str(".globl _meta_wrapper_plainfile\n_meta_wrapper_plainfile:\n    .ascii \"plainfile\"\n");
    out.push_str(".globl _meta_mode_r\n_meta_mode_r:\n    .ascii \"r\"\n");
    out.push_str(".globl _meta_mode_w\n_meta_mode_w:\n    .ascii \"w\"\n");
    out.push_str(".globl _meta_mode_rw\n_meta_mode_rw:\n    .ascii \"r+\"\n");
    out.push_str(".p2align 3\n");
    out.push_str(".globl _tmpfile_template\n_tmpfile_template:\n    .ascii \"/tmp/elephc-XXXXXX\\0\"\n    .byte 0,0,0,0,0\n");
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

    /// Verifies the configured PHP precision is embedded in the cacheable `%G` format.
    #[test]
    fn test_float_precision_is_baked_into_runtime_data() {
        let asm = emit_runtime_data_fixed(
            8_388_608,
            Target::new(Platform::MacOS, Arch::AArch64),
            13,
            5,
        );
        assert!(asm.contains(".asciz \"%.13G\""));
    }

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
            let target = Target::new(platform, arch);
            let asm = emit_runtime_data_fixed(8_388_608, target, 14, 5);

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
            Target::new(Platform::Linux, Arch::AArch64),
            14,
            5,
        );

        assert!(asm.contains(".comm _stack_limit, 8, 8\n"));
        assert!(asm.contains(".comm _stack_limit_main, 8, 8\n"));
    }

    /// The function-pointer slots a *Rust* bridge crate resolves must be spelled with the
    /// platform's C-ABI mangling, not a hardcoded Mach-O underscore.
    ///
    /// Most common symbols here are private to the emitted assembly, so their name is
    /// self-consistent whatever it is. These six are different: `elephc-pdo` and
    /// `elephc-web` declare them as `extern "C" { static … }`, so the linker looks for
    /// `_name` on Mach-O and `name` on ELF. Emitting `_name` everywhere assembles fine on
    /// both and then fails every ELF link with `undefined reference to 'elephc_probe_route_fn'`
    /// — which is how a `--web` or PDO binary stopped linking on linux-aarch64 while every
    /// macOS job stayed green. Same silent-until-link shape as the alignment sweep above.
    #[test]
    fn test_bridge_resolved_slots_use_the_platform_c_abi_mangling() {
        // Derived from the bridge crates rather than pinned here: a hand-kept list would
        // silently stop covering the seventh slot someone adds. Reading the sources at
        // test time follows the same approach as the sentinel scans in `sentinels.rs`.
        let bridge_slots = declared_bridge_slots();
        assert!(
            bridge_slots.len() >= 6,
            "expected the bridge crates to declare their runtime slots, found {bridge_slots:?}"
        );
        for (platform, arch, prefix) in [
            (Platform::MacOS, Arch::AArch64, "_"),
            (Platform::Linux, Arch::AArch64, ""),
            (Platform::Linux, Arch::X86_64, ""),
        ] {
            let target = Target::new(platform, arch);
            let asm = emit_runtime_data_fixed(8_388_608, target, 14, 5);
            for slot in &bridge_slots {
                let wanted = format!(".comm {prefix}{slot}, 8, ");
                assert!(
                    asm.contains(&wanted),
                    "{platform:?}/{arch:?} never declares `{wanted}…`; the bridge crate that \
                     resolves `{slot}` will fail to link"
                );
                // And the other spelling must be absent, or the wrong one satisfies the link
                // on one platform while the right one is missing on the other.
                let unwanted = if prefix.is_empty() {
                    format!(".comm _{slot}, ")
                } else {
                    format!(".comm {slot}, ")
                };
                assert!(
                    !asm.contains(&unwanted),
                    "{platform:?}/{arch:?} still declares `{unwanted}…`, the other platform's \
                     spelling of {slot}"
                );
            }
        }
    }

    /// Verifies PHP 8.4+ runtime defaults exclude the removed E_STRICT bit.
    #[test]
    fn test_error_reporting_default_tracks_php_profile() {
        let target = Target::new(Platform::MacOS, Arch::AArch64);
        let php83 = emit_runtime_data_fixed(8_388_608, target, 14, 3);
        let php85 = emit_runtime_data_fixed(8_388_608, target, 14, 5);
        assert!(php83.contains("_rt_error_reporting:\n    .quad 32767\n"));
        assert!(php85.contains("_rt_error_reporting:\n    .quad 30719\n"));
    }

    /// Every `elephc_*` runtime slot a bridge crate resolves through the C ABI, read from
    /// the crates themselves so the check cannot fall behind them.
    ///
    /// Matches a declaration — `static elephc_x: usize;` — and not a definition, which is
    /// how the `#[cfg(test)]` stubs that give those crates their own zero slots
    /// (`static elephc_instr_io_fn: usize = 0;`) stay out of the list: a crate that
    /// defines the symbol itself constrains nothing about the runtime's spelling.
    fn declared_bridge_slots() -> Vec<String> {
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("crates");
        let mut found = Vec::new();
        collect_extern_statics(&crates, &mut found);
        found.sort();
        found.dedup();
        found
    }

    /// Collects every runtime `.comm` symbol the bridge crates declare, by
    /// reading their sources — the test that holds the emitted set and the
    /// declared set to each other.
    fn collect_extern_statics(dir: &std::path::Path, found: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_extern_statics(&path, found);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let Ok(body) = std::fs::read_to_string(&path) else {
                    continue;
                };
                for line in body.lines() {
                    let line = line.trim();
                    let Some(rest) = line.strip_prefix("static elephc_") else {
                        continue;
                    };
                    // A declaration ends at the type; a definition carries `= …`.
                    if line.contains('=') {
                        continue;
                    }
                    if let Some((name, _)) = rest.split_once(':') {
                        found.push(format!("elephc_{}", name.trim()));
                    }
                }
            }
        }
    }
}
