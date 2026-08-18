//! Purpose:
//! Declares the Linux Magician binding for `pcntl_getcpuaffinity`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - CPU identifiers are returned as a fresh indexed runtime array.

eval_builtin! { contract: "pcntl_getcpuaffinity", area: Pcntl, direct: Pcntl, values: Pcntl }
