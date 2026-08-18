//! Purpose:
//! Binds the Linux `pcntl_unshare` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Namespace changes remain process-global and are conservatively effectful.

builtin! {
    contract: "pcntl_unshare",
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::Unshare),
}
