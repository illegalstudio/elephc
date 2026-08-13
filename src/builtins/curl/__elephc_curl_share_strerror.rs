//! Purpose:
//! Home of the internal `__elephc_curl_share_strerror` builtin: libcurl's own message for
//! a `CURLSHcode`.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_share_strerror()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - IT IS NOT `__elephc_curl_strerror`/`__elephc_curl_multi_strerror` WITH A DIFFERENT
//!   ARGUMENT. `CURLSHcode`, `CURLcode` and `CURLMcode` are three separate numbering
//!   spaces (`curl_share_strerror(2)` is "share is in use", unrelated to either sibling's
//!   code `2`), so routing one through another's table would produce a confidently wrong
//!   message.
//! - Takes NO handle, like its easy/multi siblings: a code's text depends on nothing else.

builtin! {
    name: "__elephc_curl_share_strerror",
    area: Curl,
    params: [error_code: Int],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlShareStrerror,
    ),
    summary: "Reports libcurl's message for a CURLSHcode for the curl prelude.",
    internal: true,
}
