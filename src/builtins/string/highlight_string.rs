//! Purpose:
//! Home of the PHP `highlight_string` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The renderer is the interpreter's PHP-exact highlighter, so the builtin forces the eval
//!   bridge and lowers to a Magician helper (see `runtime_features`).
//! - The result is a union (`string|bool`), stored as the boxed Mixed cell the helper returns.

builtin! {
    contract: "highlight_string",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::HighlightString,
    ),
}
