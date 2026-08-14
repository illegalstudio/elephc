//! Purpose:
//! Home of the internal `__elephc_curl_easy_getinfo_long` builtin: reads a `long`-typed
//! `CURLINFO_*` field from an easy handle's most recent transfer.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_getinfo()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - COVERS BOTH `CURLINFO_LONG` AND `CURLINFO_OFF_T`. The `$info` operand is forwarded to the
//!   bridge unchanged, which itself refuses (returns `false`) anything outside those two
//!   type ranges (`crates/elephc-curl/src/easy.rs`'s `getinfo_long`), so
//!   calling this with a non-`long`/`off_t` `CURLINFO_*` number is safe but useless. The PRELUDE
//!   dispatches to this builtin for every `CURLINFO_*` in either type mask
//!   (`crate::curl_prelude::curl_getinfo`'s `$mask === 2097152 || $mask === 6291456` branch),
//!   not just `CURLINFO_HTTP_CODE`.
//! - Returns `int|false`: the fetched value on success, or `false` for an unknown handle, a
//!   non-`long` info type, or a libcurl `getinfo` failure. Declared `Mixed` rather than
//!   `int|false` for the same checker reason `curl_version()`'s `array|false` was left
//!   undeclared (`crate::curl_prelude`'s divergence note) — here the return never touches
//!   an array, so the risk is smaller, but there is no need to invite it.

builtin! {
    name: "__elephc_curl_easy_getinfo_long",
    area: Curl,
    params: [handle: Mixed, info: Int],
    returns: Mixed,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasyGetinfoLong,
    ),
    summary: "Reads a long-typed CURLINFO field for the curl prelude.",
    internal: true,
}
