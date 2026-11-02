//! Purpose:
//! Home of the PHP `copy` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook: `copy` is a pure-data builtin whose `Bool` return type is
//!   fully determined by its declaration. The registry common path infers the
//!   arguments and enforces the 2-3 argument arity before falling back to `returns`.
//! - The optional stream `$context` is accepted and ignored: local copies have no context.


builtin! {
    contract: "copy",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Copy,
    ),
}
