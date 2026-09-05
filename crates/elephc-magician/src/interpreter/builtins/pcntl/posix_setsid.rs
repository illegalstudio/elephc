//! Purpose:
//! Declares the Magician binding for `posix_setsid`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Eval returns the bridge's session identifier or native `-1` failure sentinel.

eval_builtin! { contract: "posix_setsid", area: Pcntl, direct: Pcntl, values: Pcntl }
