//! Purpose:
//! Home of the PHP `preg_replace` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Str`) is fully determined by the declaration.


builtin! {
    name: "preg_replace",
    area: System,
    params: [pattern: Str, replacement: Str, subject: Str],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PregReplace,
    ),
    summary: "Performs a regular expression search and replace.",
}
