//! Purpose:
//! Home of the PHP `memory_get_peak_usage` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Reads the runtime's `_gc_peak` high-water counter. `$real_usage` is accepted and
//!   ignored, as in `memory_get_usage`.

builtin! {
    contract: "memory_get_peak_usage",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::MemoryGetPeakUsage,
    ),
}
