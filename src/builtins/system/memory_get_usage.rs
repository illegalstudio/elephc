//! Purpose:
//! Home of the PHP `memory_get_usage` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Reads the runtime's `_gc_live` footprint counter (maintained on every allocation and
//!   free). The `$real_usage` argument is accepted and ignored: elephc tracks a single
//!   footprint, so both values answer the same number, matching PHP's `false` (the default).

builtin! {
    contract: "memory_get_usage",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::MemoryGetUsage,
    ),
}
