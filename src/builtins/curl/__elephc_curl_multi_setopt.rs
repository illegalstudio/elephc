//! Purpose:
//! Home of the internal `__elephc_curl_multi_setopt` builtin: applies an integer-valued
//! `CURLMOPT_*` option to a multi handle.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_multi_setopt()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - IT ANSWERS A THREE-WAY CODE, not a boolean: `1` applied, `0` a real PHP option this
//!   build cannot carry (or one libcurl refused) -> `false` plus a warning, `-1` not a
//!   cURL multi option at all -> php-src's `ValueError`. That split is php-src's own, and
//!   the reason the answer is signed.
//! - THE OPTION IS CLASSIFIED INSIDE THE BRIDGE, not here and not in the prelude, because
//!   `curl_multi_setopt` is variadic and picks how to read its third argument from the
//!   option's numeric range — `CURLMOPT_PUSHFUNCTION` would read a PHP integer as a
//!   function pointer. Same memory-safety boundary `curl_setopt()`'s option table is.

builtin! {
    name: "__elephc_curl_multi_setopt",
    area: Curl,
    params: [multi: Mixed, option: Int, value: Int],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlMultiSetopt,
    ),
    summary: "Applies an integer-valued CURLMOPT option to a multi handle for the curl prelude.",
    internal: true,
}
