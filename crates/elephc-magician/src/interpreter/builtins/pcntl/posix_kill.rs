//! Purpose:
//! Declares the Magician binding for `posix_kill`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Eval delegates signal delivery to the shared native bridge.

eval_builtin! { contract: "posix_kill", area: Pcntl, direct: Pcntl, values: Pcntl }
