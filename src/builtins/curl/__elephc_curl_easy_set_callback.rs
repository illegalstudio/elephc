//! Purpose:
//! Home of the internal `__elephc_curl_easy_set_callback` builtin: installs, replaces, or
//! clears one of `curl_setopt()`'s callback options on a raw easy handle.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_setopt()` in `crate::curl_prelude`, for
//!   `CURLOPT_WRITEFUNCTION`, `CURLOPT_HEADERFUNCTION`, `CURLOPT_READFUNCTION`,
//!   `CURLOPT_PROGRESSFUNCTION`, `CURLOPT_XFERINFOFUNCTION`, and `CURLOPT_DEBUGFUNCTION`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - THE CALLABLE IS DECOMPOSED BEFORE IT GETS HERE. `$descriptor` is the pointer
//!   `__elephc_callable_ptr(__elephc_normalize_callable($value))` produced and `$adapter`
//!   is `__elephc_curl_adapter_addr()`, so no bridge extern ever declares a `callable`
//!   parameter — the same "decompose at the PHP layer" split `elephc-pdo` uses for
//!   SQLite's user functions. Passing `0` for `$descriptor` clears the slot, which is how
//!   PHP `null` restores an option's default behavior.
//! - `$self` IS THE `CurlHandle` OBJECT ITSELF, not its `$__elephc_handle` id: libcurl
//!   callbacks receive `$ch` as their first PHP argument, and the identity has to be the
//!   very object the caller holds (`$ch === $captured` is observable). The bridge stores
//!   it as a NON-OWNING back-pointer, exactly like php-src's `ch->self`; see
//!   `crates/elephc-curl/src/callbacks.rs` for why an owning reference would be an
//!   uncollectable refcount cycle.
//! - `$slot` is a small bridge-side slot index, not a `CURLOPT_*` number. The option ->
//!   slot mapping lives in the prelude next to the rest of the option dispatch, so this
//!   entry point stays one fixed-shape C call.

builtin! {
    contract: "__elephc_curl_easy_set_callback",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasySetCallback,
    ),
}
