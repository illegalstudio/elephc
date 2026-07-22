//! Purpose:
//! Home of the PHP `ob_end_flush` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Flushes the top buffer to the parent sink, then pops the stack.
//! - Pure-data builtin: returns `Bool` (`false` when no output buffer is active).


builtin! {
    name: "ob_end_flush",
    area: Io,
    params: [],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ObEndFlush,
    ),
    summary: "Flushes (sends) the contents of the active output buffer and turns it off.",
    php_manual: "function.ob-end-flush",
}
