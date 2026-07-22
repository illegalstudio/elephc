//! Purpose:
//! Home of the PHP `ob_flush` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Flushes the top buffer to the parent sink without popping it.
//! - Pure-data builtin: returns `Bool` (`false` when no output buffer is active).


builtin! {
    name: "ob_flush",
    area: Io,
    params: [],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ObFlush,
    ),
    summary: "Flushes (sends) the contents of the active output buffer.",
    php_manual: "function.ob-flush",
}
