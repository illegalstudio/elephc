//! Purpose:
//! Home of PHP's `error_reporting` builtin declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Omitting the nullable level reads the active mask; an integer updates it and
//!   returns the previous mask.

builtin! {
    contract: "error_reporting",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ErrorReporting,
    ),
}
