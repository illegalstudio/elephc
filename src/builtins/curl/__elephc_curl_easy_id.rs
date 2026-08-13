//! Purpose:
//! Home of the internal `__elephc_curl_easy_id` builtin: the bridge id an easy handle's
//! boxed Mixed cell already carries, as a plain PHP `int`.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP bodies of `curl_multi_add_handle()`/`curl_multi_remove_handle()`/
//!   `curl_multi_info_read()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - IT EXISTS FOR ONE REASON: HANDLE IDENTITY. `curl_multi_info_read()` must return the
//!   SAME `CurlHandle` OBJECT that was added to the multi handle (php-src's
//!   `_php_curl_multi_find_easy_handle`), and libcurl reports a completed transfer by its
//!   native handle, which the bridge translates to its own `i64` id. The prelude therefore
//!   keys a PHP-side id -> object map on this value; without it there would be no way to
//!   get from the ABI's id back to the object, and wrapping a FRESH `CurlHandle` around
//!   the id would give two objects one native handle and double-free it.
//! - IT CALLS NOTHING. The id is the payload of the handle's own Mixed cell, so the
//!   lowering is the unbox and nothing else — no bridge entry point, no allocation.

builtin! {
    name: "__elephc_curl_easy_id",
    area: Curl,
    params: [handle: Mixed],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasyId,
    ),
    summary: "Reads a libcurl easy handle's bridge id for the curl prelude's identity map.",
    internal: true,
}
