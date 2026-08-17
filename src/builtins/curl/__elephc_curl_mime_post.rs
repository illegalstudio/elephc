//! Purpose:
//! Home of the internal `__elephc_curl_mime_post` builtin: attaches the pending
//! `curl_mime` builder to an easy handle via `CURLOPT_MIMEPOST`, completing the
//! `CURLOPT_POSTFIELDS` array walk.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP array walker in `crate::curl_prelude`, once the whole array has been
//!   walked successfully.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - Replaces whatever mime was previously ATTACHED (if any) only after libcurl accepts
//!   the new one; on failure the pending builder is freed and the previous attachment (if
//!   any) is left untouched.

builtin! {
    contract: "__elephc_curl_mime_post",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlMimePost,
    ),
}
