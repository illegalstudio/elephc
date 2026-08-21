//! Purpose:
//! Home of the internal `__elephc_curl_mime_add_part` builtin: appends a fresh, empty part
//! to the pending `curl_mime` builder, becoming the target of every following
//! `__elephc_curl_mime_part_field` call.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP array walker in `crate::curl_prelude`, once per `CURLOPT_POSTFIELDS`
//!   array item.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - Fails when `__elephc_curl_mime_new` was never called for this handle.

builtin! {
    contract: "__elephc_curl_mime_add_part",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlMimeAddPart,
    ),
}
