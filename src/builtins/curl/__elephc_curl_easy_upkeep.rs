//! Purpose:
//! Home of the internal `__elephc_curl_easy_upkeep` builtin: runs libcurl's connection
//! upkeep (keepalive pings) on an easy handle's idle connections.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_upkeep()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - `true` MEANS `CURLE_OK`, matching PHP's `curl_upkeep(): bool`. The bridge does the
//!   comparison, so this builtin's answer is already a clean `0`/`1`.

builtin! {
    contract: "__elephc_curl_easy_upkeep",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasyUpkeep,
    ),
}
