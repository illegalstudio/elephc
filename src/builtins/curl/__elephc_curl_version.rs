//! Purpose:
//! Home of the internal `__elephc_curl_version` builtin: hands back the linked libcurl's
//! `curl_version_info(CURLVERSION_NOW)` data as a JSON blob for the prelude to decode.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_version()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - WHY JSON AND NOT AN ARRAY. PHP's `curl_version()` returns an associative array whose
//!   value types are mixed (ints, strings, and a list of strings) and whose key set
//!   depends on which sub-libraries this libcurl build reports. Building that array in
//!   assembly would mean a bespoke, per-target array constructor for one function;
//!   emitting the bridge's JSON and decoding it with the ordinary `json_decode()` builtin
//!   reuses a decoder that already produces exactly the right PHP types, on every target,
//!   with no new runtime surface at all.
//! - The values describe the libcurl that is actually LINKED, read at run time from the
//!   library itself — never a constant captured at compile time, which is what makes the
//!   pinned-version assertion in `tests/codegen/curl/easy_handle.rs` meaningful.
//! - `""` means the bridge could not produce the blob (it is not linked, or the buffer
//!   probe failed); the prelude turns that into PHP's `false`.

builtin! {
    name: "__elephc_curl_version",
    area: Curl,
    params: [],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlVersion,
    ),
    summary: "Reports the linked libcurl's version info as JSON for the curl prelude.",
    internal: true,
}
