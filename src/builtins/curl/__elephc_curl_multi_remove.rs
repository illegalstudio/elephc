//! Purpose:
//! Home of the internal `__elephc_curl_multi_remove` builtin: detaches an easy handle from
//! a multi handle, answering libcurl's `CURLMcode`.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_multi_remove_handle()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - A `CURLMcode`, not a boolean, for the same reason `__elephc_curl_multi_add` is.
//! - DETACHING DOES NOT FREE THE EASY HANDLE. libcurl leaves it fully usable (it can be
//!   re-added, or run through `curl_exec()`), and the `CurlHandle` object keeps owning it,
//!   so nothing about this call touches the ownership chain.

builtin! {
    contract: "__elephc_curl_multi_remove",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlMultiRemove,
    ),
}
