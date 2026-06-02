//! Purpose:
//! Models optimizer side effects for PHP builtin calls.
//! Feeds purity, callable alias, builtin, and call-effect decisions into pruning and dead-code elimination.
//!
//! Called from:
//! - `crate::optimize::effects`
//!
//! Key details:
//! - Effect summaries must account for globals, heap/runtime state, output, throws, and by-reference mutation.

/// Returns `true` if the named PHP builtin function is both pure (no side effects)
/// and non-throwing (does not emit warnings or fatal errors that could alter control flow).
///
/// Pure non-throwing builtins are safe to eliminate via dead-code elimination, fold as
/// constant expressions when all arguments are constant, and reorder freely relative to
/// other statements. The list intentionally excludes builtins that read/write shared runtime
/// state (`json_*`), produce observable side effects (output, filesystem, globals, heap
/// mutation), or can throw/fatal (pointer helpers, etc.).
///
/// # Arguments
/// * `name` - Lowercase ASCII builtin function name (case-insensitive PHP builtins use
///   lowercase in the catalog; callers must normalize before calling).
///
/// # Returns
/// `true` if the builtin is pure and non-throwing; `false` otherwise.
///
/// # Notes
/// `json_encode`/`json_decode`/`json_validate`/`json_last_error`/`json_last_error_msg`
/// are excluded because they read/write the shared `_json_last_error` runtime symbol.
/// Pointer memory helpers (`ptr_read16`, `ptr_write16`, `ptr_read_string`, `ptr_write_string`)
/// are excluded because raw memory access can null-dereference or fatal.
pub(super) fn is_pure_non_throwing_builtin(name: &str) -> bool {
    matches!(
        name,
        "strlen"
            | "count"
            | "intval"
            | "floatval"
            | "boolval"
            | "gettype"
            | "is_array"
            | "is_bool"
            | "is_float"
            | "is_int"
            | "is_null"
            | "is_numeric"
            | "is_string"
            | "is_resource"
            | "get_resource_type"
            | "get_resource_id"
            | "abs"
            | "min"
            | "max"
            | "floor"
            | "ceil"
            | "round"
            | "sqrt"
            | "pow"
            | "fmod"
            | "fdiv"
            | "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "atan2"
            | "deg2rad"
            | "rad2deg"
            | "sinh"
            | "cosh"
            | "tanh"
            | "log"
            | "log2"
            | "log10"
            | "exp"
            | "hypot"
            | "pi"
            | "number_format"
            | "substr"
            | "strpos"
            | "strrpos"
            | "strstr"
            | "str_replace"
            | "str_ireplace"
            | "substr_replace"
            | "strtolower"
            | "strtoupper"
            | "ucfirst"
            | "lcfirst"
            | "ucwords"
            | "trim"
            | "ltrim"
            | "rtrim"
            | "chop"
            | "str_repeat"
            | "strrev"
            | "grapheme_strrev"
            | "str_pad"
            | "explode"
            | "implode"
            | "str_split"
            | "strcmp"
            | "strcasecmp"
            | "str_contains"
            | "str_starts_with"
            | "str_ends_with"
            | "ord"
            | "chr"
            | "nl2br"
            | "wordwrap"
            | "addslashes"
            | "stripslashes"
            | "htmlspecialchars"
            | "htmlentities"
            | "html_entity_decode"
            | "urlencode"
            | "urldecode"
            | "rawurlencode"
            | "rawurldecode"
            | "md5"
            | "sha1"
            | "crc32"
            | "hash"
            | "base64_encode"
            | "base64_decode"
            | "bin2hex"
            | "hex2bin"
            | "long2ip"
            | "ip2long"
            | "inet_ntop"
            | "inet_pton"
            | "ctype_alpha"
            | "ctype_digit"
            | "ctype_alnum"
            | "ctype_space"
            | "array_key_exists"
            | "array_search"
            | "array_keys"
            | "array_values"
            | "array_merge"
            | "array_slice"
            | "array_combine"
            | "array_flip"
            | "array_reverse"
            | "array_unique"
            | "array_column"
            | "array_sum"
            | "array_product"
            | "array_chunk"
            | "array_pad"
            | "array_fill"
            | "array_fill_keys"
            | "array_diff"
            | "array_intersect"
            | "array_diff_key"
            | "array_intersect_key"
            | "range"
    )
    // Note: json_encode / json_decode / json_validate / json_last_error /
    // json_last_error_msg are intentionally NOT listed here — they read
    // and write the shared `_json_last_error` runtime symbol, so the
    // optimizer must treat them as side-effecting to avoid DCE-ing an
    // encode/decode call right before a json_last_error() observation.
    // Pointer memory helpers such as ptr_read16(), ptr_write16(),
    // ptr_read_string(), and ptr_write_string() are also intentionally absent:
    // raw memory reads/writes and null/length fatals must remain observable.
}
