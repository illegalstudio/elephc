//! Purpose:
//! Home of the internal `__elephc_sodium_status` builtin: the status the last
//! `__elephc_sodium_box` call recorded, which the sodium prelude maps to PHP's
//! `SodiumException` or `false`.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP bodies in `crate::sodium_prelude`.
//!
//! Key details:
//! - Reads a runtime data slot; it has no arguments and never allocates.

builtin! {
    contract: "__elephc_sodium_status",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::SodiumStatus,
    ),
}
