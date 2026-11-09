//! Purpose:
//! Home of the PHP `preg_last_error` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `preg_last_error` takes no arguments and always
//!   returns `Int`. The value is the `_preg_last_error` runtime slot every `preg_*`
//!   call clears and the regex runtime sets on failure.

builtin! {
    contract: "preg_last_error",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PregLastError,
    ),
}
