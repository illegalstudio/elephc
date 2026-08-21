//! Purpose:
//! Home of the internal `__elephc_curl_setopt_unsupported_warning` builtin: raises PHP's
//! warning for a real `CURLOPT_*` option this build cannot apply safely, on the `false`
//! path such an option always answers.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_setopt()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - WHY THIS EXISTS AT ALL. Decision 7 forbids answering an unsupported option with an
//!   inert `true`; it must return `false` AND emit PHP's warning. The prelude is ordinary
//!   elephc-PHP and has no other way to reach the runtime's diagnostic channel, so the
//!   warning gets a builtin of its own rather than a bespoke `fwrite(STDERR, ...)` that
//!   would bypass `display_errors` handling.
//! - IT DECLARES NO BRIDGE REQUIREMENT, unlike every other builtin in this area: it only
//!   formats a string through `__rt_diag_warning`, never touching libcurl. Declaring one
//!   would be inaccurate, and it is unnecessary in practice — reaching this call at all
//!   means a `CurlHandle` already exists, which already pulled the bridge in.

builtin! {
    contract: "__elephc_curl_setopt_unsupported_warning",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlSetoptUnsupportedWarning,
    ),
}
