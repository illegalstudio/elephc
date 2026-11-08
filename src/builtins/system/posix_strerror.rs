//! Purpose:
//! Binds the shared `posix_strerror` contract to its typed POSIX error-message EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Lowering copies libc-owned message bytes into fresh PHP string storage.

builtin! {
    contract: "posix_strerror",
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::PosixStrError),
}
