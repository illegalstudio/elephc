//! Purpose:
//! Binds the Linux `pcntl_getcpuaffinity` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - The result is boxed because PHP returns either an indexed integer array or false.

builtin! {
    contract: "pcntl_getcpuaffinity",
    check: super::pcntl_linux_support::check_getcpuaffinity,
    semantics: crate::builtins::semantics::pcntl_semantics(
        crate::ir::PcntlRuntime::GetCpuAffinity,
    ),
}
