//! Purpose:
//! Home of the internal `__elephc_curl_easy_setopt_null` builtin: clears a nullable
//! string-valued `curl_setopt()` option by forwarding libcurl's own `NULL`.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_setopt()` in `crate::curl_prelude`.
//!
//! Key details:
//! - php-src's nullable string options (`CURLOPT_CUSTOMREQUEST`, `CURLOPT_ACCEPT_ENCODING`,
//!   `CURLOPT_RANGE`, …) reset to their default when handed `null`, and it does that by
//!   passing a NULL `char *` to `curl_easy_setopt` — an empty string is NOT the same thing.
//!   The ordinary string setter cannot express NULL, so this entry point exists.

builtin! {
    contract: "__elephc_curl_easy_setopt_null",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasySetoptNull,
    ),
}
