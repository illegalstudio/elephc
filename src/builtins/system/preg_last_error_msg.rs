//! Purpose:
//! Home of the PHP `preg_last_error_msg` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Returns php's message for `preg_last_error()` ("No error", "Internal error", ...) from a
//!   static table in the PCRE2 shim, so the result is never heap storage.

builtin! {
    contract: "preg_last_error_msg",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PregLastErrorMsg,
    ),
}
