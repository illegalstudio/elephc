//! Purpose:
//! Home of the internal `__elephc_diag_suppressed` builtin: the current `@` suppression depth.
//!
//! Called from:
//! - The error-handling prelude's `error_reporting()` (`src/error_handling_prelude.rs`).
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so no user program can call it.
//! - Reads `_rt_diag_suppression`, the counter `@` pushes and pops, with no allocation.

builtin! {
    contract: "__elephc_diag_suppressed",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcDiagSuppressed,
    ),
}
