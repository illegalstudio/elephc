//! Purpose:
//! Declares the Magician binding for `posix_geteuid`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Eval returns the bridge's widened effective user id.

eval_builtin! { contract: "posix_geteuid", area: Pcntl, direct: Pcntl, values: Pcntl }
