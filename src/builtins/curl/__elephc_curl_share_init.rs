//! Purpose:
//! Home of the internal `__elephc_curl_share_init` builtin: allocates a libcurl SHARE
//! handle and hands back the boxed Mixed cell that owns it.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_share_init()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - OWNERSHIP: the returned cell is a resource-kind-8 Mixed handle (the share sibling of
//!   the easy handle's kind 6 and the multi handle's kind 7) and is the ONLY owner of the
//!   native share handle. Its release path is `__rt_mixed_free_deep` ->
//!   `__rt_curl_share_free` -> `curl_share_cleanup` — except when the entry is a
//!   PERSISTENT share (`curl_share_init_persistent()`, PHP 8.5), for which
//!   `elephc_curl_share_free` is a documented no-op (see `crates/elephc-curl/src/share.rs`).
//!   `CurlShareHandle` deliberately has no `__destruct` and `curl_share_close()` is a
//!   no-op, exactly as in PHP 8, so there is exactly one free path.
//! - `false` ON ALLOCATION FAILURE, which the prelude turns into a thrown
//!   `RuntimeException` — php-src's `curl_share_init(): CurlShareHandle` has no `false`
//!   arm to answer through, the same divergence `curl_multi_init()` already documents.

builtin! {
    contract: "__elephc_curl_share_init",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlShareInit,
    ),
}
