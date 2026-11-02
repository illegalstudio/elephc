//! Purpose:
//! Binds the shared `posix_kill` contract to its typed signal-delivery EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Both the process identifier and the signal are passed through the stable PCNTL bridge as
//!   widened integers.

builtin! {
    contract: "posix_kill",
    semantics: crate::builtins::semantics::pcntl_semantics(
        crate::ir::PcntlRuntime::Kill,
    ),
}
