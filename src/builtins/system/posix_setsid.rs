//! Purpose:
//! Binds the shared `posix_setsid` contract to its typed session-creation EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - The bridge returns the new session identifier or the native `-1` failure sentinel.

builtin! {
    contract: "posix_setsid",
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::SetSession),
}
