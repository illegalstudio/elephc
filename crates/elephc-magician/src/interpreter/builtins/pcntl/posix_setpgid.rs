//! Purpose:
//! Declares the Magician binding for `posix_setpgid`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Eval delegates process-group mutation to the shared native bridge.

eval_builtin! { contract: "posix_setpgid", area: Pcntl, direct: Pcntl, values: Pcntl }
