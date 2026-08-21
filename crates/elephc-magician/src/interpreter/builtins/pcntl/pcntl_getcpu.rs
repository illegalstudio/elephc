//! Purpose:
//! Declares the Linux Magician binding for `pcntl_getcpu`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Registry availability hides this function on macOS.

eval_builtin! { contract: "pcntl_getcpu", area: Pcntl, direct: Pcntl, values: Pcntl }
