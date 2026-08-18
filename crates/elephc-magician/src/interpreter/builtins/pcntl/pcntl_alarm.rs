//! Purpose:
//! Declares the Magician binding for `pcntl_alarm`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Execution is delegated to the shared PCNTL dispatcher.

eval_builtin! { contract: "pcntl_alarm", area: Pcntl, direct: Pcntl, values: Pcntl }
