//! Purpose:
//! Declares the Magician binding for `pcntl_async_signals`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Automatic dispatch state is owned by the persistent eval context.

eval_builtin! { contract: "pcntl_async_signals", area: Pcntl, direct: Pcntl, values: Pcntl }
