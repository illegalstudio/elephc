//! Purpose:
//! Emits fixed JSON literal and escape lookup data for JSON runtime helpers.
//! The data symbols are consumed by encoder and decoder state machines across target emitters.
//!
//! Called from:
//! - `crate::codegen::runtime::system::emit_json_data()` during fixed data emission.
//!
//! Key details:
//! - Data symbol names are consumed directly by JSON helpers and must not drift from scanner logic.

/// Emits fixed JSON literal constants and error-message lookup data as assembly.
///
/// Returns a `String` containing `.globl`, `.ascii`, `.quad`, and `.balign`
/// directives for the following symbols:
///
/// - `_json_true`, `_json_false`, `_json_null` — JSON keywords
/// - `_json_int_max_str`, `_json_int_min_str` — overflow thresholds for
///   `json_decode`'s `JSON_BIGINT_AS_STRING` logic (i64 bounds)
/// - `_json_err_msg_0` through `_json_err_msg_10` — per-error human-readable
///   messages matching PHP's `json_last_error_msg()` exactly
/// - `_json_err_msg_table` — parallel ptr/len pairs indexed by error code
/// - `_json_err_msg_count` — total error message count
///
/// The table uses `.balign 8` to ensure 8-byte alignment after the
/// variable-length `.ascii` strings. The pointer/length layout allows a
/// single bounds-checked load to materialize a string slice in the runtime
/// helper.
pub(crate) fn emit_json_data() -> String {
    let mut out = String::new();
    out.push_str(".globl _json_true\n_json_true:\n    .ascii \"true\"\n");
    out.push_str(".globl _json_false\n_json_false:\n    .ascii \"false\"\n");
    out.push_str(".globl _json_null\n_json_null:\n    .ascii \"null\"\n");

    // Lexicographic-compare thresholds for JSON_BIGINT_AS_STRING in
    // json_decode. Both strings have the same length as the longest
    // representable PHP_INT (i64) magnitude in decimal. Byte-wise lex
    // compare on equal-length, leading-zero-free decimal strings agrees
    // with numeric compare, so the decoder uses these to detect overflow
    // without trial-parsing through __rt_atoi (which silently wraps).
    out.push_str(".globl _json_int_max_str\n_json_int_max_str:\n    .ascii \"9223372036854775807\"\n");
    out.push_str(".globl _json_int_min_str\n_json_int_min_str:\n    .ascii \"-9223372036854775808\"\n");

    // Per-error-code human-readable messages, indexed by JSON_ERROR_*.
    // The strings match PHP's json_last_error_msg() exactly so cross-checking
    // with `php -r` produces identical output.
    let messages: &[(&str, &str)] = &[
        ("_json_err_msg_0", "No error"),
        ("_json_err_msg_1", "Maximum stack depth exceeded"),
        ("_json_err_msg_2", "State mismatch (invalid or malformed JSON)"),
        ("_json_err_msg_3", "Control character error, possibly incorrectly encoded"),
        ("_json_err_msg_4", "Syntax error"),
        ("_json_err_msg_5", "Malformed UTF-8 characters, possibly incorrectly encoded"),
        ("_json_err_msg_6", "Recursion detected"),
        ("_json_err_msg_7", "Inf and NaN cannot be JSON encoded"),
        ("_json_err_msg_8", "Type is not supported"),
        ("_json_err_msg_9", "The decoded property name is invalid"),
        (
            "_json_err_msg_10",
            "Single unpaired UTF-16 surrogate in unicode escape",
        ),
    ];

    for (label, message) in messages {
        out.push_str(&format!(
            ".globl {label}\n{label}:\n    .ascii \"{message}\"\n",
            label = label,
            message = message,
        ));
    }

    // Parallel ptr/len table indexed by error code so the runtime helper does
    // a single bounds-checked load to materialize a string slice. The .balign
    // directive ensures 8-byte alignment after the variable-length .ascii
    // strings above.
    out.push_str("    .balign 8\n");
    out.push_str(".globl _json_err_msg_table\n");
    out.push_str("_json_err_msg_table:\n");
    for (label, message) in messages {
        out.push_str(&format!(
            "    .quad {label}\n    .quad {len}\n",
            label = label,
            len = message.len(),
        ));
    }

    out.push_str("    .balign 8\n");
    out.push_str(&format!(
        ".globl _json_err_msg_count\n_json_err_msg_count:\n    .quad {}\n",
        messages.len(),
    ));

    out
}
