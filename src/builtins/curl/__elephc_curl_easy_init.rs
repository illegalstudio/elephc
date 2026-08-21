//! Purpose:
//! Home of the internal `__elephc_curl_easy_init` builtin: allocates a libcurl easy
//! handle and hands back the opaque Mixed cell the curl prelude stores in
//! `CurlHandle::$__elephc_handle`.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_init()` in `crate::curl_prelude`.
//!
//! Key details:
//! - WHY IT IS INTERNAL. PHP 8's `curl_init()` returns a `CurlHandle` OBJECT, not a
//!   resource. elephc models that object as an ordinary elephc-PHP class declared by
//!   `crate::curl_prelude`, and a prelude function cannot shadow a builtin of the same
//!   name (`Cannot redeclare built-in function`). Renaming the raw entry point frees
//!   `curl_init` for the prelude wrapper.
//! - IT TAKES NO `$url`, unlike PHP's `curl_init(?string $url = null)`. The bridge's C ABI
//!   allocates and sets the URL through two separate entry points
//!   (`elephc_curl_easy_init` / `elephc_curl_easy_set_url`), and the prelude wrapper
//!   already has to branch on `$url === null` to reproduce PHP's "no URL set at all"
//!   state, so the branch lives in PHP where it is readable rather than in assembly.
//! - The result is `false` when libcurl cannot allocate a handle (PHP's own failure
//!   answer for `curl_init()`), otherwise the boxed handle. It never reaches user code
//!   directly and consumes no PHP resource id (see `runtime::resource_ids`): a
//!   `CurlHandle` is counted in the OBJECT handle space, so it does not shift the ids of
//!   surrounding `fopen()` streams.

builtin! {
    contract: "__elephc_curl_easy_init",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasyInit,
    ),
}
