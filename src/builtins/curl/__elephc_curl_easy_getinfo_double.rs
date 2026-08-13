//! Purpose:
//! Home of the internal `__elephc_curl_easy_getinfo_double` builtin: reads a
//! `double`-typed `CURLINFO_*` field from an easy handle's most recent transfer.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_getinfo()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_getinfo_long` for the type-mask reasoning; this is its
//!   `CURLINFO_DOUBLE` sibling, and the bridge refuses any `info` outside that range
//!   rather than asking libcurl to write a `double` through a differently-shaped pointer.
//! - Returns `float|false`, declared `Mixed` for the same checker reason its `long`
//!   sibling documents.

builtin! {
    name: "__elephc_curl_easy_getinfo_double",
    area: Curl,
    params: [handle: Mixed, info: Int],
    returns: Mixed,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasyGetinfoDouble,
    ),
    summary: "Reads a double-typed CURLINFO field for the curl prelude.",
    internal: true,
}
