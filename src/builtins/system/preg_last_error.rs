//! Purpose:
//! Home of the PHP `preg_last_error` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Reads the PCRE2 shim's code for the most recent compile or match (0 none, 1 internal, 2
//!   backtrack limit, 3 recursion limit, 4 bad UTF-8, 5 bad UTF-8 offset), the values of php's
//!   `PREG_*_ERROR` constants.

builtin! {
    contract: "preg_last_error",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PregLastError,
    ),
}
