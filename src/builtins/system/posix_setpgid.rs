//! Purpose:
//! Binds the shared `posix_setpgid` contract to its typed process-group EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Both identifiers are passed through the stable PCNTL bridge as widened integers.

builtin! {
    contract: "posix_setpgid",
    semantics: crate::builtins::semantics::pcntl_semantics(
        crate::ir::PcntlRuntime::SetProcessGroup,
    ),
}
