//! Purpose:
//! Binds the shared `pcntl_sigprocmask` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - The optional old-mask output is write-only and may create a new local.

builtin! {
    contract: "pcntl_sigprocmask",
    check: super::pcntl_signal_support::check_sigprocmask,
    lazy_check: true,
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::SignalMask),
        crate::builtins::semantics::BuiltinArgumentLowering::PcntlPreserveOmitted,
    ),
}
