//! Purpose:
//! Declares the internal suppression-aware runtime diagnostic builtin.
//!
//! Called from:
//! - Synthetic builtin class methods that must reproduce PHP runtime warnings or deprecations.
//!
//! Key details:
//! - The complete formatted diagnostic is passed as a string to `__rt_diag_warning`.
//! - Optional source-line and error-level operands append location and apply the
//!   active `error_reporting()` mask before emitting.
//! - The builtin is internal and therefore absent from PHP-visible function discovery.

builtin! {
    contract: "__elephc_diag_warning",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcDiagWarning,
    ),
}
