//! Purpose:
//! Home of the PHP `http_get_last_response_headers` builtin: its single-source
//! registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - php 8.4 added this as the replacement for the `$http_response_header`
//!   variable, which php 8.5 deprecates. Its return type is `?array`.
//! - The nullable half is load-bearing, not cosmetic: MEASURED on `php -n` 8.5.6
//!   the function answers `NULL` before the first request and again after
//!   `http_clear_last_response_headers()`, where `$http_response_header` would be
//!   an undefined variable. `Mixed` is the return type that carries both the
//!   `null` and the indexed array through one boxed cell, so the hook keeps the
//!   contract's `Mixed` rather than narrowing to `Array<Str>`.

builtin! {
    contract: "http_get_last_response_headers",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::HttpGetLastResponseHeaders,
    ),
}
