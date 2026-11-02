//! Purpose:
//! Home of the internal `__elephc_last_error_int` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - This is an internal builtin (`internal: true`) not exposed as a PHP-visible function.
//!   `error_handling_prelude` reads the native last-diagnostic record through it: field 0 is
//!   the level (0 when nothing is recorded), 1 the line, and 2 clears the record.

builtin! {
    contract: "__elephc_last_error_int",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcLastErrorInt,
    ),
}
