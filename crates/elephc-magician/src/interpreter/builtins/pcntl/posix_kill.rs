//! Purpose:
//! Declares the Magician binding for `posix_kill`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Eval raises PHP's process-id ValueError through the shared bridge classification.

eval_builtin! { contract: "posix_kill", area: Pcntl, direct: Pcntl, values: Pcntl }
