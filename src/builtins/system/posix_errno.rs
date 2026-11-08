//! Purpose:
//! Binds the shared `posix_errno` contract to its typed POSIX last-error EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - This alias reads the same ext/posix last-error state as `posix_get_last_error()`.

builtin! {
    contract: "posix_errno",
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::PosixGetLastError),
}
