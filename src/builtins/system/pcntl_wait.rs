//! Purpose:
//! Binds the shared `pcntl_wait` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - The child status is written back through the required first by-reference parameter.

builtin! {
    contract: "pcntl_wait",
    check: super::pcntl_wait_support::check_wait,
    lazy_check: true,
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::Wait),
        crate::builtins::semantics::BuiltinArgumentLowering::PcntlPreserveOmitted,
    ),
}
