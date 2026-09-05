//! Purpose:
//! Binds the Linux `pcntl_sigtimedwait` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Timeout is validated before the stable bridge call and timeout returns boxed false.

builtin! {
    contract: "pcntl_sigtimedwait",
    check: super::pcntl_signal_support::check_sigtimedwait,
    lazy_check: true,
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::SignalTimedWait),
        crate::builtins::semantics::BuiltinArgumentLowering::PcntlPreserveOmitted,
    ),
}
