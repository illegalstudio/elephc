//! Purpose:
//! Registers `memory_get_allocations()` against the shared native heap counters.
//!
//! Called from:
//! - The builtin registry through `crate::builtins::system`.
//!
//! Key details:
//! - Uses a typed, target-aware runtime operation shared with eval.

builtin! {
    contract: "memory_get_allocations",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::MemoryGetAllocations,
    ),
}
