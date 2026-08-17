//! Purpose:
//! Home of the internal `__elephc_curl_multi_exec` builtin: drives a multi handle's
//! attached transfers, answering the still-running count and the `CURLMcode` PACKED into
//! one integer.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_multi_exec()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - THE PACKING IS WHAT LETS A BY-REFERENCE OUT-PARAMETER STAY IN PHP. PHP's
//!   `curl_multi_exec(CurlMultiHandle $mh, int &$still_running): int` produces TWO values;
//!   a builtin produces one. The bridge returns `(running << 32) | (code & 0xFFFFFFFF)`
//!   and the prelude unpacks both halves, so the by-reference write happens in elephc-PHP
//!   where it is an ordinary assignment, and no second entry point or pointer
//!   out-parameter is needed. `crates/elephc-curl/src/multi.rs` owns the layout.
//! - ITS RUNTIME HELPER DOES NOT WIDEN THE ANSWER: the value is already a full `int64_t`,
//!   and sign- or zero-extending it would destroy the high half.

builtin! {
    contract: "__elephc_curl_multi_exec",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlMultiExec,
    ),
}
