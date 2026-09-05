//! Purpose:
//! Declares the Magician binding for `pcntl_setpriority`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Optional process and selector values use the shared PHP signature.

eval_builtin! { contract: "pcntl_setpriority", area: Pcntl, direct: Pcntl, values: Pcntl }
