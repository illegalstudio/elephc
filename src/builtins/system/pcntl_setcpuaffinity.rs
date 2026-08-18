//! Purpose:
//! Binds the Linux `pcntl_setcpuaffinity` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - The checker keeps the CPU mask as a non-empty indexed integer array.

builtin! {
    contract: "pcntl_setcpuaffinity",
    check: super::pcntl_linux_support::check_setcpuaffinity,
    semantics: crate::builtins::semantics::pcntl_semantics(
        crate::ir::PcntlRuntime::SetCpuAffinity,
    ),
}
