//! Purpose:
//! Binds the shared `pcntl_strerror` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Lowering copies libc-owned message bytes into fresh PHP string storage.

builtin! {
    contract: "pcntl_strerror",
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::StrError),
}
