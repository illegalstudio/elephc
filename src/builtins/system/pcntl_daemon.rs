//! Purpose:
//! Binds Elephc's `pcntl_daemon` extension to its typed PCNTL EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - The shared contract hides this non-PHP convenience API under `--strict-php`.

builtin! {
    contract: "pcntl_daemon",
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::Daemon),
}
