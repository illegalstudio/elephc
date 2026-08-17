//! Purpose:
//! Home of the internal `__elephc_curl_share_setopt` builtin: applies an integer-valued
//! `CURLSHOPT_*` option (`CURLSHOPT_SHARE`/`CURLSHOPT_UNSHARE`) to a share handle.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_share_setopt()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - IT ANSWERS A THREE-WAY CODE, mirroring `__elephc_curl_multi_setopt`: `1` applied, `0`
//!   a real `CURLSHOPT_SHARE`/`UNSHARE` call libcurl itself refused (an unrecognized
//!   `CURL_LOCK_DATA_*` value) -> `false`, `-1` not a cURL share option at all -> php-src's
//!   `ValueError`. UNLIKE `__elephc_curl_multi_setopt`, the `0` answer here is NEVER "a
//!   real option this build cannot carry": php-src's own `curl_share_setopt()` switch has
//!   exactly two cases (`CURLSHOPT_SHARE`/`UNSHARE`), so there is no third bucket to warn
//!   about — see `crates/elephc-curl/src/share.rs`'s module doc.
//! - THE OPTION IS CLASSIFIED INSIDE THE BRIDGE for the same memory-safety reason
//!   `curl_setopt()`/`curl_multi_setopt()`'s tables are: `curl_share_setopt` is variadic.

builtin! {
    contract: "__elephc_curl_share_setopt",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlShareSetopt,
    ),
}
