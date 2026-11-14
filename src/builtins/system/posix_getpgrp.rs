//! Purpose:
//! Binds the shared `posix_getpgrp` contract to its typed process-group EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - The bridge widens libc's `pid_t` to a PHP integer.

builtin! {
    contract: "posix_getpgrp",
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::PosixGetPgrp),
}
