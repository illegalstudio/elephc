//! Purpose:
//! Declares the Magician binding for `pcntl_waitid`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Default id type, id, and flags match the shared PHP contract.

eval_builtin! { contract: "pcntl_waitid", area: Pcntl, direct: Pcntl, values: Pcntl }
