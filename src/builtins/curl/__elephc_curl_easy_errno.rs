//! Purpose:
//! Home of the internal `__elephc_curl_easy_errno` builtin: reports the `CURLcode` from
//! the most recent transfer on a raw easy handle.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_errno()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - This is the ONE place in the `elephc_curl` C ABI whose integer return is not a
//!   boolean: it hands back the raw `CURLcode` by design, because it exists specifically
//!   to back PHP's `curl_errno()`. A handle that never performed a transfer reports `0`
//!   (`CURLE_OK`), matching PHP's initial state for a fresh handle.

builtin! {
    name: "__elephc_curl_easy_errno",
    area: Curl,
    params: [handle: Mixed],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasyErrno,
    ),
    summary: "Reports the last libcurl error code for the curl prelude.",
    internal: true,
}
