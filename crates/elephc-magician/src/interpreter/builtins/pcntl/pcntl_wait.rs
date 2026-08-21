//! Purpose:
//! Declares the Magician binding for `pcntl_wait`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Status and optional resource usage preserve caller writeback.

eval_builtin! { contract: "pcntl_wait", area: Pcntl, direct: Pcntl, values: Pcntl }
