//! Purpose:
//! Binds the shared `pcntl_signal` contract to callable-aware PCNTL EIR lowering.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Integer dispositions and AOT callable descriptors share one target-neutral handler table.

builtin! {
    contract: "pcntl_signal",
    check: super::pcntl_signal_support::check_signal,
    lazy_check: true,
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::Signal),
}
