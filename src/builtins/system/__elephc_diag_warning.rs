//! Purpose:
//! Declares the internal suppression-aware runtime diagnostic builtin.
//!
//! Called from:
//! - Synthetic builtin class methods that must reproduce PHP runtime warnings or deprecations.
//!
//! Key details:
//! - The complete formatted diagnostic is passed as a string to `__rt_diag_warning`.
//! - The builtin is internal and therefore absent from PHP-visible function discovery.

builtin! {
    name: "__elephc_diag_warning",
    area: System,
    params: [message: Str],
    returns: Void,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcDiagWarning,
    ),
    summary: "Emits one suppression-aware PHP runtime diagnostic.",
    internal: true,
}
