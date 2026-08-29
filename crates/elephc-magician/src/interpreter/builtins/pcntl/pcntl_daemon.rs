//! Purpose:
//! Declares the Magician binding for Elephc's `pcntl_daemon` extension.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - The surviving daemon process continues through the shared PCNTL bridge.

eval_builtin! { contract: "pcntl_daemon", area: Pcntl, direct: Pcntl, values: Pcntl }
