//! Purpose:
//! Binds the Linux `pcntl_sigwaitinfo` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Success returns a boxed signal number and writes a heterogeneous siginfo array.

builtin! {
    contract: "pcntl_sigwaitinfo",
    check: super::pcntl_signal_support::check_sigwaitinfo,
    lazy_check: true,
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::SignalWaitInfo),
        crate::builtins::semantics::BuiltinArgumentLowering::PcntlPreserveOmitted,
    ),
}
