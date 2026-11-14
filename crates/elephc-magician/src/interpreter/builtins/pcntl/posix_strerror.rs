//! Purpose:
//! Declares the Magician binding for `posix_strerror`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Eval copies libc's borrowed message into a fresh PHP string.

eval_builtin! { contract: "posix_strerror", area: Pcntl, direct: Pcntl, values: Pcntl }
