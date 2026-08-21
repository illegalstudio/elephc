//! Purpose:
//! Home of the internal `__elephc_curl_multi_init` builtin: allocates a libcurl MULTI
//! handle and hands back the boxed Mixed cell that owns it.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_multi_init()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - OWNERSHIP: the returned cell is a resource-kind-7 Mixed handle (the multi sibling of
//!   the easy handle's kind 6) and is the ONLY owner of the native multi handle. Its
//!   release path is `__rt_mixed_free_deep` -> `__rt_curl_multi_free` ->
//!   `curl_multi_cleanup`. `CurlMultiHandle` deliberately has no `__destruct` and
//!   `curl_multi_close()` is a no-op, exactly as in PHP 8, so there is exactly one free.
//! - `false` ON ALLOCATION FAILURE, which the prelude turns into a thrown
//!   `RuntimeException` — php-src's `curl_multi_init(): CurlMultiHandle` cannot answer
//!   `false`, so returning one would be a signature it does not have.

builtin! {
    contract: "__elephc_curl_multi_init",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlMultiInit,
    ),
}
