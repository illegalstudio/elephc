//! Purpose:
//! Binds the shared `pcntl_async_signals` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Omitting the nullable flag queries state; supplying it returns the prior state.

builtin! {
    contract: "pcntl_async_signals",
    semantics: crate::builtins::semantics::pcntl_semantics(
        crate::ir::PcntlRuntime::AsyncSignals,
    ),
}
