//! Purpose:
//! Binds the Linux `pcntl_setns` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Omitted or null process identifiers select the current process in the bridge.

builtin! {
    contract: "pcntl_setns",
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::SetNs),
}
