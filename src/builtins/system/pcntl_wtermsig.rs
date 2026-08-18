//! Purpose:
//! Binds the shared `pcntl_wtermsig` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - The public mixed contract follows PHP's platform-sensitive declaration.

builtin! {
    contract: "pcntl_wtermsig",
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::WTermSig),
}
