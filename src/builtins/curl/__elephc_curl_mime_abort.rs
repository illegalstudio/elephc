//! Purpose:
//! Home of the internal `__elephc_curl_mime_abort` builtin: discards the pending
//! `curl_mime` builder without attaching it.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP array walker in `crate::curl_prelude`, when a `CURLOPT_POSTFIELDS`
//!   array item's shape is not supported (a nested array, an unrecognized object) or a
//!   field is refused, so the half-built structure this crate owns is never leaked and
//!   whatever mime is already ATTACHED from an earlier successful call is left untouched.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - Always succeeds, including when there is no pending builder: this is a cleanup call,
//!   not a status query.

builtin! {
    name: "__elephc_curl_mime_abort",
    area: Curl,
    params: [handle: Mixed],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlMimeAbort,
    ),
    summary: "Discards the pending curl_mime builder without attaching it for the curl prelude.",
    internal: true,
}
