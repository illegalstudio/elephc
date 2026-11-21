//! Purpose:
//! Binds the shared `posix_get_last_error` contract to its typed POSIX last-error EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - ext/posix keeps its own errno, separate from `pcntl_get_last_error()`, and success never clears it.

builtin! {
    contract: "posix_get_last_error",
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::PosixGetLastError),
}
