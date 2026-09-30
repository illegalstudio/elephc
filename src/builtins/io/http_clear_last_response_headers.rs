//! Purpose:
//! Home of the PHP `http_clear_last_response_headers` builtin: its single-source
//! registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - php 8.4's companion to `http_get_last_response_headers()`. Its return type is
//!   `void`, so the contract's `returns: Void` is authoritative and no check hook
//!   is needed.
//! - It clears ENGINE state, not the `$http_response_header` variable: MEASURED on
//!   `php -n` 8.5.6, a clear makes the getter answer `NULL` again while an already
//!   populated `$http_response_header` keeps its value.

builtin! {
    contract: "http_clear_last_response_headers",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::HttpClearLastResponseHeaders,
    ),
}
