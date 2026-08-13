//! Purpose:
//! Home of the internal `__elephc_curl_multi_select` builtin: waits (up to a millisecond
//! timeout) for one of a multi handle's attached transfers to become ready.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_multi_select()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - THE TIMEOUT ARRIVES IN MILLISECONDS, already converted from PHP's `float $timeout`
//!   (seconds) by the prelude — the same conversion php-src does inline
//!   (`(int)(timeout * 1000.0)`).
//! - IT ANSWERS THE NUMBER OF READY DESCRIPTORS, or `-1` when libcurl reported an error,
//!   which is PHP's own contract; the runtime helper sign-extends so `-1` stays `-1`.

builtin! {
    name: "__elephc_curl_multi_select",
    area: Curl,
    params: [multi: Mixed, timeout_ms: Int],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlMultiSelect,
    ),
    summary: "Waits for a libcurl multi handle's transfers to be ready, for the curl prelude.",
    internal: true,
}
