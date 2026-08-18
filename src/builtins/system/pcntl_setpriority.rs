//! Purpose:
//! Binds the shared `pcntl_setpriority` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Optional process-id and selector arguments retain PHP's zero defaults.

builtin! {
    contract: "pcntl_setpriority",
    semantics: crate::builtins::semantics::pcntl_semantics(
        crate::ir::PcntlRuntime::SetPriority,
    ),
}
