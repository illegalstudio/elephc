//! Purpose:
//! Home of the internal `__elephc_curl_multi_errno` builtin: the `CURLMcode` from a multi
//! handle's most recent operation.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_multi_errno()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - A `CURLMcode`, sign-extended by its runtime helper, exactly like
//!   `__elephc_curl_easy_errno`'s `CURLcode` — and a DIFFERENT numbering space from it, so
//!   the two must never be formatted through each other's `strerror`.

builtin! {
    contract: "__elephc_curl_multi_errno",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlMultiErrno,
    ),
}
