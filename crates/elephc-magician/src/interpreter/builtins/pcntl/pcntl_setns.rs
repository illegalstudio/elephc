//! Purpose:
//! Declares the Linux Magician binding for `pcntl_setns`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Null process identifiers and omitted namespace types use PHP defaults.

eval_builtin! { contract: "pcntl_setns", area: Pcntl, direct: Pcntl, values: Pcntl }
