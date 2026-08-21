//! Purpose:
//! Home of the internal `__elephc_curl_easy_reset` builtin: resets every libcurl option on
//! an easy handle to its default and clears the PHP-layer state that goes with them.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_reset()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - THE HANDLE STAYS THE SAME OBJECT. `curl_reset()` in PHP does not mint a new session:
//!   the `CurlHandle` keeps its identity and its live connections, only the options go
//!   back to default — which is why this is a per-handle call rather than a re-init.
//! - The bridge reinstalls elephc's own write callback and error buffer afterwards
//!   (`curl_easy_reset` clears them, since they are options too); the prelude clears the
//!   object-side `RETURNTRANSFER` and `CURLOPT_PRIVATE` mirrors.

builtin! {
    contract: "__elephc_curl_easy_reset",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasyReset,
    ),
}
