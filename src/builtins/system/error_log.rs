//! Purpose:
//! Home of PHP's `error_log` builtin and its stderr-oriented AOT runtime contract.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Optional delivery arguments are accepted for PHP signature compatibility; the AOT runtime
//!   currently writes every message to stderr and reports success.

builtin! {
    contract: "error_log",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ErrorLog,
    ),
}
