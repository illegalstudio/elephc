//! Purpose:
//! Home of the internal `__elephc_curl_strerror` builtin: libcurl's own human-readable
//! text for a `CURLcode`.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_strerror()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - IT TAKES NO HANDLE, unlike every other per-transfer curl builtin: a `CURLcode`'s text
//!   is a property of the LIBRARY, so the bridge answers it without consulting the handle
//!   table, the same way `__elephc_curl_version` does.
//! - The message is copied into an owned PHP string through a fixed stack buffer in the
//!   runtime helper, sized by `CURL_ERROR_SIZE` — the same bound `__rt_curl_easy_error`
//!   relies on, and comfortably above every message libcurl ships.

builtin! {
    contract: "__elephc_curl_strerror",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlStrerror,
    ),
}
