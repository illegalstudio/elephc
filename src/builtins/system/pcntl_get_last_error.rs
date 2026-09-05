//! Purpose:
//! Binds `pcntl_get_last_error` to the typed last-PCNTL-error EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - The returned value is the thread-local errno captured by bridge operations.

builtin! {
    contract: "pcntl_get_last_error",
    semantics: crate::builtins::semantics::pcntl_semantics(
        crate::ir::PcntlRuntime::GetLastError,
    ),
}
