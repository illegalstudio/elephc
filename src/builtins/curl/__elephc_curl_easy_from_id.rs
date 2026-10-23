//! Purpose:
//! Home of the internal `__elephc_curl_easy_from_id` builtin: boxes a bridge easy-handle
//! id that libcurl created itself (a server-pushed handle) as a resource-kind-6 Mixed
//! cell, so PHP can wrap it in a `CurlHandle`.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - `__elephc_curl_push_invoke()` in `crate::curl_prelude`, which receives the pushed
//!   handle only as an id from the bridge's push trampoline.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal, and for the
//!   resource-kind-6 boxing this shares with it. The difference is ownership of the
//!   native handle: `easy_init` allocates it, this one adopts an id that already exists
//!   (libcurl owns the pushed `CURL*` and frees it with the multi handle).
//! - Id `0` is the bridge's "no handle" answer and boxes to PHP `false`, exactly like
//!   every other handle producer, so the prelude's null check stays uniform.
//! - It calls no bridge entry point: the id is already registered, and the box only
//!   carries it. The free pointer publication still happens, because a pushed handle's
//!   Mixed cell is released through the same kind-6 teardown as any other `CurlHandle`.

builtin! {
    contract: "__elephc_curl_easy_from_id",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasyFromId,
    ),
}
