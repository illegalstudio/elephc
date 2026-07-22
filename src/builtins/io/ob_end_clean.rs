//! Purpose:
//! Home of the PHP `ob_end_clean` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Discards the top buffer and pops the stack.
//! - Pure-data builtin: returns `Bool` (`false` when no output buffer is active).


builtin! {
    name: "ob_end_clean",
    area: Io,
    params: [],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ObEndClean,
    ),
    summary: "Cleans (erases) the contents of the active output buffer and turns it off.",
    php_manual: "function.ob-end-clean",
}
