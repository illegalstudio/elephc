//! Purpose:
//! Binds the shared `pcntl_waitpid` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - The selected child status is written back through the required second by-reference parameter.

builtin! {
    contract: "pcntl_waitpid",
    check: super::pcntl_wait_support::check_waitpid,
    lazy_check: true,
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::WaitPid),
        crate::builtins::semantics::BuiltinArgumentLowering::PcntlPreserveOmitted,
    ),
}
