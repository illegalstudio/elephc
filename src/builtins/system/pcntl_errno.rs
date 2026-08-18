//! Purpose:
//! Binds the `pcntl_errno` alias to the typed last-PCNTL-error EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - This alias deliberately shares runtime state and lowering with `pcntl_get_last_error`.

builtin! {
    contract: "pcntl_errno",
    semantics: crate::builtins::semantics::pcntl_semantics(
        crate::ir::PcntlRuntime::GetLastError,
    ),
}
