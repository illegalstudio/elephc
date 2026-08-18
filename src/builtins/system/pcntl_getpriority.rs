//! Purpose:
//! Binds the shared `pcntl_getpriority` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - The mixed result distinguishes a successful priority of `-1` from failure.

builtin! {
    contract: "pcntl_getpriority",
    semantics: crate::builtins::semantics::pcntl_semantics(
        crate::ir::PcntlRuntime::GetPriority,
    ),
}
