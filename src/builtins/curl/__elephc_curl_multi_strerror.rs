//! Purpose:
//! Home of the internal `__elephc_curl_multi_strerror` builtin: libcurl's own message for
//! a `CURLMcode`.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_multi_strerror()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - IT IS NOT `__elephc_curl_strerror` WITH A DIFFERENT ARGUMENT. `CURLMcode` and
//!   `CURLcode` are separate numbering spaces: `curl_multi_strerror(3)` is "out of memory"
//!   while `curl_strerror(3)` is "URL using bad/illegal format", so routing one through
//!   the other's table would produce a confidently wrong message.
//! - Takes NO handle, like its easy sibling: a code's text depends on nothing else.

builtin! {
    contract: "__elephc_curl_multi_strerror",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlMultiStrerror,
    ),
}
