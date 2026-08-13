//! Purpose:
//! Home of the internal `__elephc_curl_easy_setopt_str` builtin: applies a
//! string-valued `curl_setopt()` option to a raw easy handle.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP bodies of `curl_init()` and `curl_setopt()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_setopt_long` for why the string and long setters are
//!   separate builtins rather than one `mixed`-valued one.
//! - The value is a byte string, not required to be UTF-8: a URL or a header line is
//!   just a NUL-free byte string as far as libcurl cares. An embedded NUL makes the
//!   bridge answer `false` (it cannot build the `CString` libcurl requires) rather than
//!   silently truncating at the NUL.

builtin! {
    name: "__elephc_curl_easy_setopt_str",
    area: Curl,
    params: [handle: Mixed, option: Int, value: Str],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasySetoptStr,
    ),
    summary: "Applies a string-valued libcurl option for the curl prelude.",
    internal: true,
}
