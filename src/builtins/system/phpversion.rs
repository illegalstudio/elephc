//! Purpose:
//! Home of the PHP `phpversion` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with zero parameters: return type (`Str`) is fully determined
//!   by the declaration. elephc returns the compiler package version string.


builtin! {
    name: "phpversion",
    area: System,
    params: [],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Phpversion,
    ),
    summary: "Returns the current PHP / elephc compiler version string.",
}
