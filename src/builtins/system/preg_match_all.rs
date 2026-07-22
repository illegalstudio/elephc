//! Purpose:
//! Home of the PHP `preg_match_all` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Int`) is fully determined by the declaration.


builtin! {
    name: "preg_match_all",
    area: System,
    params: [pattern: Str, subject: Str],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PregMatchAll,
    ),
    summary: "Performs a global regular expression match and returns the number of matches.",
}
