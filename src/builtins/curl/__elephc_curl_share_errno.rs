//! Purpose:
//! Home of the internal `__elephc_curl_share_errno` builtin: the `CURLSHcode` from a share
//! handle's most recent operation.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_share_errno()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - A `CURLSHcode`, sign-extended by its runtime helper (its range includes no negative
//!   values today, but it travels the same width as `curl_errno()`/`curl_multi_errno()`
//!   for consistency) — a DIFFERENT numbering space from either of them, so the two must
//!   never be formatted through each other's `strerror`.

builtin! {
    contract: "__elephc_curl_share_errno",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlShareErrno,
    ),
}
