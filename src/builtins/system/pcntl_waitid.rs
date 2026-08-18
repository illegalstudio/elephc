//! Purpose:
//! Binds the shared `pcntl_waitid` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - The optional signal-information output is write-only and may create a new local.

builtin! {
    contract: "pcntl_waitid",
    check: super::pcntl_wait_support::check_waitid,
    lazy_check: true,
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::WaitId),
}
