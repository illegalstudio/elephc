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
//! - THE LIFETIME HAZARD THIS BUILTIN IS PART OF CLOSING: libcurl requires a share object
//!   to outlive every easy handle attached to it, including through that easy handle's own
//!   eventual `curl_easy_cleanup()` — freeing the share first is a real use-after-free
//!   unless something detaches every attached easy handle first. `crates/elephc-curl/src/
//!   share.rs`'s module doc carries the full argument; the short version is that the
//!   BRIDGE, not this PHP-level call, is the source of truth: `elephc_curl_share_free`
//!   walks every id this builtin's ABI entry point (`elephc_curl_easy_set_share`) has
//!   recorded as attached and clears `CURLOPT_SHARE` on each before `curl_share_cleanup`,
//!   so no PHP-side strong reference from `CurlHandle` to `CurlShareHandle` is needed on
//!   top of it.

builtin! {
    name: "__elephc_curl_easy_set_share",
    area: Curl,
    params: [handle: Mixed, share: Mixed],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasySetShare,
    ),
    summary: "Attaches a libcurl easy handle to a share handle for curl_setopt()'s CURLOPT_SHARE.",
    internal: true,
}
