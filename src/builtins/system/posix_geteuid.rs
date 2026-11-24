//! Purpose:
//! Binds the shared `posix_geteuid` contract to its typed effective-user-id EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - The bridge widens libc's `uid_t` to a PHP integer.

builtin! {
    contract: "posix_geteuid",
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::PosixGetEuid),
}
