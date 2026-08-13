//! Purpose:
//! Home of the internal `__elephc_curl_easy_str_op` builtin: runs one of the bridge's
//! string-producing easy-handle operations and hands back its bytes as a PHP string, or
//! `false` when the bridge could not answer.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP bodies of `curl_getinfo()`, `curl_escape()` and `curl_unescape()` in
//!   `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - ONE BUILTIN, SEVERAL OPERATIONS, DELIBERATELY. Every string-shaped answer the bridge
//!   produces needs the identical two-step dance: run the operation, then copy the bytes
//!   out of the bridge's borrowed buffer before the next call can overwrite them. Each
//!   operation as its own builtin would mean re-writing that dance in hand-written
//!   assembly for three targets, once per operation, with nothing else different. The
//!   `$op` selector (defined in `crates/elephc-curl/src/abi.rs`) is the whole of the
//!   indirection that buys it back.
//! - `$text` is the operation's string argument (empty for the `curl_getinfo()` ops) and
//!   `$number` its integer one (the `CURLINFO_*` value for those same ops).
//! - Returns `string|false`, declared `Mixed` for the same checker reason
//!   `__elephc_curl_easy_getinfo_long` documents. `false` is a real PHP `false`, so the
//!   prelude's `=== false` checks work without special-casing.

builtin! {
    name: "__elephc_curl_easy_str_op",
    area: Curl,
    params: [handle: Mixed, op: Int, text: Str, number: Int],
    returns: Mixed,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasyStrOp,
    ),
    summary: "Runs a string-producing libcurl easy-handle operation for the curl prelude.",
    internal: true,
}
