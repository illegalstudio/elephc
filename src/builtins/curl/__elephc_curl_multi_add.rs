//! Purpose:
//! Home of the internal `__elephc_curl_multi_add` builtin: attaches an easy handle to a
//! multi handle, answering libcurl's `CURLMcode`.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_multi_add_handle()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - IT RETURNS A `CURLMcode`, NOT A BOOLEAN — `0` (`CURLM_OK`) is success and PHP's
//!   `curl_multi_add_handle(): int` hands the code straight back. The runtime helper
//!   therefore SIGN-extends the bridge's `int32_t`, like `curl_errno()`'s.
//! - THE BRIDGE ALSO CLEARS THE EASY HANDLE'S CAPTURE BUFFER here, mirroring php-src's
//!   `_php_curl_cleanup_handle` on the same call; see `crates/elephc-curl/src/multi.rs`.

builtin! {
    name: "__elephc_curl_multi_add",
    area: Curl,
    params: [multi: Mixed, handle: Mixed],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlMultiAdd,
    ),
    summary: "Attaches a libcurl easy handle to a multi handle for the curl prelude.",
    internal: true,
}
