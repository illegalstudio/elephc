//! Purpose:
//! Registers `memory_get_usage()` against the shared native heap counters.
//!
//! Called from:
//! - The builtin registry through `crate::builtins::system`.
//!
//! Key details:
//! - Uses a typed, target-aware runtime operation shared with eval.

builtin! {
    contract: "memory_get_usage",
    semantics: super::memory_support::query_semantics(
        crate::ir::RuntimeFnId::MemoryGetUsage,
    ),
}
