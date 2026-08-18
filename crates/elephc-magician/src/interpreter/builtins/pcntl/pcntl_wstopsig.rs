//! Purpose:
//! Declares the Magician binding for `pcntl_wstopsig`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Target-native wait status decoding remains in libc.

eval_builtin! { contract: "pcntl_wstopsig", area: Pcntl, direct: Pcntl, values: Pcntl }
