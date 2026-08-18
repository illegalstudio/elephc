//! Purpose:
//! Binds the shared `pcntl_signal_dispatch` contract to queued callback dispatch.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Callback invocation runs outside the operating-system signal-handler context.

builtin! {
    contract: "pcntl_signal_dispatch",
    semantics: crate::builtins::semantics::pcntl_semantics(
        crate::ir::PcntlRuntime::SignalDispatch,
    ),
}
