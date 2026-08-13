//! Purpose:
//! Home of the internal `__elephc_curl_easy_perform` builtin: runs the configured
//! transfer on a raw easy handle to completion.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_exec()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - PHP's `curl_exec()` has THREE return shapes (`false` on failure, the body `string`
//!   under `CURLOPT_RETURNTRANSFER`, `true` otherwise), and which of the two success
//!   shapes applies depends on an option the Task-3 C ABI exposes no getter for. This
//!   builtin therefore answers only "did the transfer succeed"; the prelude wrapper
//!   picks the success shape from the `CurlHandle` object's own record of
//!   `CURLOPT_RETURNTRANSFER` and calls `__elephc_curl_easy_body` when it applies.
//! - Without `CURLOPT_RETURNTRANSFER` the response body is written to stdout by the
//!   bridge's write callback, matching PHP CLI, so a successful default transfer has
//!   already produced its output by the time this returns.

builtin! {
    name: "__elephc_curl_easy_perform",
    area: Curl,
    params: [handle: Mixed],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasyPerform,
    ),
    summary: "Runs a libcurl transfer to completion for the curl prelude.",
    internal: true,
}
