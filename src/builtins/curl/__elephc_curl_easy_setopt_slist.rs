//! Purpose:
//! Home of the internal `__elephc_curl_easy_setopt_slist` builtin: applies a
//! `struct curl_slist *`-valued `curl_setopt()` option (`CURLOPT_HTTPHEADER`,
//! `CURLOPT_QUOTE`, `CURLOPT_RESOLVE`, ...) to a raw easy handle.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_setopt()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - THE LIST TRAVELS AS ONE NUL-FRAMED BLOB, not as a PHP array. Marshalling a PHP array
//!   through the C ABI would need either a second entry point per element or an array
//!   walker in the runtime helper; instead the prelude concatenates `item . "\0"` per
//!   element and the bridge splits the blob back apart (`elephc_curl_easy_setopt_slist`).
//!   That framing is unambiguous where a separator-joined one is not: `[]` is `""` and
//!   `[""]` is a single NUL byte, and PHP strings are binary-safe, so the blob survives
//!   the trip intact.
//! - OWNERSHIP OF THE BUILT LIST IS THE BRIDGE'S, not libcurl's: libcurl stores the
//!   pointer and walks it during the transfer without copying, so the list must outlive
//!   every `curl_exec()` on the handle. `EasyEntry::slists` owns it and frees it on reset
//!   or teardown.

builtin! {
    name: "__elephc_curl_easy_setopt_slist",
    area: Curl,
    params: [handle: Mixed, option: Int, items: Str],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasySetoptSlist,
    ),
    summary: "Applies a string-list libcurl option for the curl prelude.",
    internal: true,
}
