//! Purpose:
//! Home of the internal `__elephc_curl_multi_setopt_unsupported_warning` builtin: raises
//! PHP's warning for a real `CURLMOPT_*` option this build cannot apply, on the `false`
//! path such an option always answers.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_multi_setopt()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal, and
//!   `__elephc_curl_setopt_unsupported_warning` for why the warning needs a builtin at all.
//! - IT IS NOT THE EASY WARNING WITH ANOTHER ARGUMENT. PHP names the function that refused
//!   the option, so a `curl_multi_setopt()` refusal that printed `curl_setopt():` would
//!   point a reader at a call that never happened. Only the message prefix differs.
//! - IT DECLARES NO BRIDGE REQUIREMENT, for the same reason its easy sibling declares none:
//!   it formats a string through `__rt_diag_warning` and never touches libcurl.

builtin! {
    name: "__elephc_curl_multi_setopt_unsupported_warning",
    area: Curl,
    params: [option: Int],
    returns: Void,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlMultiSetoptUnsupportedWarning,
    ),
    summary: "Warns that a curl multi option is unsupported by this build, for the curl prelude.",
    internal: true,
}
