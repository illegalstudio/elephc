//! Purpose:
//! Home of the PHP `ctype_lower` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The shared `ctype_semantics()` descriptor keeps the declared `Bool` result and resolves
//!   effects from the argument type: strings are pure, other values raise PHP's deprecation.

builtin! {
    contract: "ctype_lower",
    semantics: crate::builtins::ctype::ctype_semantics(
        crate::ir::RuntimeFnId::CtypeLower,
    ),
}
