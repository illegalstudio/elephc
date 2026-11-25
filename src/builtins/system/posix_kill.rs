//! Purpose:
//! Binds the shared `posix_kill` contract to its typed signal-delivery EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - An out-of-range process id raises PHP's `ValueError`; the signal is truncated to a C int as in PHP.

builtin! {
    contract: "posix_kill",
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::PosixKill),
}
