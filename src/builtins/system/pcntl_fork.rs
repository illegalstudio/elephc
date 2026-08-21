//! Purpose:
//! Binds the shared `pcntl_fork` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Parent, child, and failure return values are passed through unchanged.

builtin! {
    contract: "pcntl_fork",
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::Fork),
}
