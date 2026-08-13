//! Purpose:
//! Home of the internal `__elephc_curl_easy_body` builtin: takes ownership of the
//! `CURLOPT_RETURNTRANSFER`-captured response body from the last transfer.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_exec()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_perform` for why `curl_exec()` is split into a perform and
//!   a body-fetch step.
//! - The bridge hands back a borrowed pointer that stays valid only until the next
//!   `elephc_curl_*` call on the same handle, so the lowering copies the bytes out
//!   immediately through `__rt_str_persist`. The resulting PHP string is independently
//!   owned and can outlive the handle.
//! - A handle that captured nothing answers `""`, which is also what PHP returns for a
//!   successful transfer with an empty body. The two are indistinguishable at this level
//!   BY DESIGN: the prelude only calls this when the handle is capturing, and an empty
//!   captured body really is `""`.

builtin! {
    name: "__elephc_curl_easy_body",
    area: Curl,
    params: [handle: Mixed],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasyBody,
    ),
    summary: "Takes the captured response body from a libcurl handle for the curl prelude.",
    internal: true,
}
