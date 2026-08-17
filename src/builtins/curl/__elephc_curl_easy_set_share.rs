//! Purpose:
//! Home of the internal `__elephc_curl_easy_set_share` builtin: attaches an easy handle to
//! a share handle via `CURLOPT_SHARE`, backing `curl_setopt($ch, CURLOPT_SHARE, $sh)`.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_setopt()` in `crate::curl_prelude` (the `CURLOPT_SHARE`
//!   branch, reached once `__elephc_curl_option_kind()` answers `KIND_SHARE`).
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - IT RETURNS A PLAIN BOOLEAN (`0`/`1`), unlike `__elephc_curl_multi_add`'s `CURLMcode`:
//!   `curl_setopt()`'s own contract is a boolean, and `CURLOPT_SHARE` is just one more
//!   `curl_setopt()` option as far as PHP is concerned, even though its VALUE is an object
//!   rather than a scalar and its bridge entry point therefore takes two handle ids
//!   instead of one.
//! - THE LIFETIME QUESTION THIS BUILTIN IS PART OF CLOSING: libcurl 8.21.0 REFCOUNTS a
//!   share (a genuine `CURLOPT_SHARE` link increments it; an easy handle's own close
//!   decrements it), so `curl_share_cleanup()` while an easy handle still references it
//!   does not corrupt anything — it fails (`CURLSHE_IN_USE`) and frees nothing, a silent
//!   PERMANENT LEAK if that failure is ignored, not a use-after-free. `crates/elephc-curl/
//!   src/share.rs`'s module doc carries the full argument; the short version is that the
//!   BRIDGE, not this PHP-level call, is the source of truth: every id this builtin's ABI
//!   entry point (`elephc_curl_easy_set_share`) records as attached is this crate's own
//!   mirror of libcurl's refcount, and `elephc_curl_share_free` DEFERS the real
//!   `curl_share_cleanup()` call (never forcibly clearing any attached easy handle's
//!   `CURLOPT_SHARE`) until that count reaches zero — so no PHP-side strong reference from
//!   `CurlHandle` to `CurlShareHandle` is needed on top of it.

builtin! {
    contract: "__elephc_curl_easy_set_share",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasySetShare,
    ),
}
