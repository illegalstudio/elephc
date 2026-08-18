//! Purpose:
//! Declares the Magician binding for `pcntl_strerror`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Borrowed libc text is copied into a fresh PHP string cell.

eval_builtin! { contract: "pcntl_strerror", area: Pcntl, direct: Pcntl, values: Pcntl }
