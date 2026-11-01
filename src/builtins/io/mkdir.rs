//! Purpose:
//! Home of the PHP `mkdir` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The declaration exposes PHP's optional permissions, recursive, and stream
//!   context arguments while the runtime semantic target remains shared by
//!   direct calls and first-class callable consumers.

builtin! {
    contract: "mkdir",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Mkdir,
    ),
}
