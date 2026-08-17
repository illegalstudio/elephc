//! Purpose:
//! Home of the internal `__elephc_curl_easy_error` builtin: reports libcurl's own error
//! message for the most recent transfer on a raw easy handle.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_error()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - The message comes from libcurl's `CURLOPT_ERRORBUFFER`, which is `CURL_ERROR_SIZE`
//!   (256) bytes including the terminating NUL, so the lowering reads it into a
//!   fixed-size stack buffer rather than sizing one at run time.
//! - A handle that never failed reports `""`, matching PHP: libcurl only guarantees to
//!   write the buffer on error, and the bridge zeroes it before every transfer so a
//!   previous failure's text can never be reported as a later success's.

builtin! {
    contract: "__elephc_curl_easy_error",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasyError,
    ),
}
