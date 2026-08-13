//! Purpose:
//! Home of the internal `__elephc_curl_easy_pause` builtin: applies a `CURLPAUSE_*`
//! bitmask to an easy handle's transfer.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_pause()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - IT RETURNS A `CURLcode`, NOT A BOOLEAN, exactly as PHP's `curl_pause(): int` does —
//!   which is why its runtime helper SIGN-extends the bridge's `int32_t` the way
//!   `__elephc_curl_easy_errno`'s does, rather than zero-extending it like the boolean
//!   forwarders.
//! - PAUSING ONLY MEANS ANYTHING MID-TRANSFER, i.e. from inside a callback (Task 12).
//!   Called on an idle handle libcurl answers `CURLE_OK` and nothing happens, which is
//!   also what php-src reports; the option is accepted here so that code written against
//!   PHP behaves the same, not because this build can pause a blocking `curl_exec()`.

builtin! {
    name: "__elephc_curl_easy_pause",
    area: Curl,
    params: [handle: Mixed, flags: Int],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasyPause,
    ),
    summary: "Applies a CURLPAUSE bitmask to a libcurl easy handle for the curl prelude.",
    internal: true,
}
