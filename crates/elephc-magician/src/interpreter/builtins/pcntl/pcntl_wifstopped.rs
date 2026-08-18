//! Purpose:
//! Declares the Magician binding for `pcntl_wifstopped`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Target-native wait status decoding remains in libc.

eval_builtin! { contract: "pcntl_wifstopped", area: Pcntl, direct: Pcntl, values: Pcntl }
