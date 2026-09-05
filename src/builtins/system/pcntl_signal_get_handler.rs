//! Purpose:
//! Binds the shared `pcntl_signal_get_handler` contract to PCNTL handler-table lookup.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Lookup returns an owned copy of the original PHP handler shape, not its invocation descriptor.

builtin! {
    contract: "pcntl_signal_get_handler",
    semantics: crate::builtins::semantics::pcntl_semantics(
        crate::ir::PcntlRuntime::SignalGetHandler,
    ),
}
