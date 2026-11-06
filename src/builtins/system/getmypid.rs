//! Purpose:
//! Home of the PHP `getmypid` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `getmypid` takes no arguments and returns `Int`. The backend
//!   reads the id from libc `getpid()` on every call, so a `pcntl_fork()` child sees its own id.

builtin! {
    contract: "getmypid",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Getmypid,
    ),
}
