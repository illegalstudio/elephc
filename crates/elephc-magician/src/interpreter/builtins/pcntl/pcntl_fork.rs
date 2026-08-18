//! Purpose:
//! Declares the Magician binding for `pcntl_fork`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Forking is performed by the same panic-free bridge used by AOT code.

eval_builtin! { contract: "pcntl_fork", area: Pcntl, direct: Pcntl, values: Pcntl }
