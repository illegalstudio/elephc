//! Purpose:
//! Binds the shared `pcntl_signal_get_handler` contract to PCNTL handler-table lookup.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Callable descriptors are retained before they are boxed into the returned `Mixed` value.

builtin! {
    contract: "pcntl_signal_get_handler",
    semantics: crate::builtins::semantics::pcntl_semantics(
        crate::ir::PcntlRuntime::SignalGetHandler,
    ),
}
