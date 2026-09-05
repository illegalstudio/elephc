//! Purpose:
//! Binds the Linux `pcntl_getcpu` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Availability is restricted by the semantic descriptor to supported Linux targets.

builtin! {
    contract: "pcntl_getcpu",
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::GetCpu),
}
