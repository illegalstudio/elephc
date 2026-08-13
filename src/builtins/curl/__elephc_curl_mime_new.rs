//! Purpose:
//! Home of the internal `__elephc_curl_mime_new` builtin: starts a fresh `curl_mime`
//! builder for an easy handle's forthcoming `multipart/form-data` `CURLOPT_POSTFIELDS`
//! body.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP array walker in `crate::curl_prelude` that replaces the array form of
//!   `curl_setopt(..., CURLOPT_POSTFIELDS, $array)`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - Discards any earlier PENDING builder (never posted, never aborted); does not touch
//!   whatever mime is already ATTACHED from an earlier successful call.

builtin! {
    name: "__elephc_curl_mime_new",
    area: Curl,
    params: [handle: Mixed],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlMimeNew,
    ),
    summary: "Starts a fresh curl_mime builder for the curl prelude's multipart POSTFIELDS walker.",
    internal: true,
}
