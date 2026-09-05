//! Purpose:
//! Binds the shared `pcntl_wifcontinued` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - The operation decodes the native child-status representation for the target.

builtin! {
    contract: "pcntl_wifcontinued",
    semantics: crate::builtins::semantics::pcntl_semantics(
        crate::ir::PcntlRuntime::WIfContinued,
    ),
}
