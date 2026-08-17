//! Purpose:
//! Home of the internal `__elephc_curl_adapter_addr` builtin: it materializes the address
//! of `__rt_curl_invoke_callback`, the shared codegen adapter that re-enters compiled PHP
//! when libcurl calls one of `curl_setopt()`'s callback options.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_setopt()` in `crate::curl_prelude`, which pairs the
//!   address with `__elephc_callable_ptr()`'s descriptor pointer and hands both to
//!   `__elephc_curl_easy_set_callback`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - It takes NO argument, unlike its PDO sibling `__elephc_pdo_adapter_addr($kind)`:
//!   there is exactly one curl adapter. All six callback options share it because the
//!   per-callback shaping (which arguments, which return convention, which libcurl abort
//!   code) is plain Rust in the bridge, and only the two things Rust cannot do — boxing a
//!   PHP object and invoking a callable descriptor — happen in the adapter.
//! - This builtin reaches no `elephc_curl_*` symbol at all, so it deliberately does NOT
//!   declare the `elephc_curl` bridge requirement; every caller of it also calls
//!   `__elephc_curl_easy_set_callback`, which does.

builtin! {
    contract: "__elephc_curl_adapter_addr",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlAdapterAddr,
    ),
}
