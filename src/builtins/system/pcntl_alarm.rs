//! Purpose:
//! Binds the shared `pcntl_alarm` contract to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - The bridge call preserves the platform alarm API's unsigned return bits.

builtin! {
    contract: "pcntl_alarm",
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::Alarm),
}
