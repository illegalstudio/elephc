//! Purpose:
//! Declares the Magician binding for `pcntl_signal`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Callable cells are retained in the persistent eval context.

eval_builtin! { contract: "pcntl_signal", area: Pcntl, direct: Pcntl, values: Pcntl }
