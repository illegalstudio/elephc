//! Purpose:
//! Declares the Linux Magician binding for `pcntl_sigwaitinfo`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Direct calls retain by-reference siginfo writeback.

eval_builtin! { contract: "pcntl_sigwaitinfo", area: Pcntl, direct: Pcntl, values: Pcntl }
