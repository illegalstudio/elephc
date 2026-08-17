//! Purpose:
//! Home of the internal `__elephc_curl_multi_info_read` builtin: reads a multi handle's
//! completion queue one field at a time.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_multi_info_read()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - ONE FIELD PER CALL, BY DESIGN. `curl_multi_info_read` pops a message off libcurl's
//!   queue destructively, so `$field = 0` performs the pop (answering `1` when there was a
//!   message, `0` when the queue is empty) and parks it on the bridge's entry; fields 1-4
//!   then read `msg`, `result`, the easy handle's bridge id, and the remaining queue
//!   length off the PARKED copy. The prelude assembles PHP's
//!   `['msg' => …, 'result' => …, 'handle' => …]` array from those integers, which is what
//!   keeps this builtin an ordinary `int` return instead of an array-building runtime
//!   helper. `crates/elephc-curl/src/multi.rs` documents the field codes.

builtin! {
    contract: "__elephc_curl_multi_info_read",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlMultiInfoRead,
    ),
}
