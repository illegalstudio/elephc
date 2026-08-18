//! Purpose:
//! Declares the Magician binding for `pcntl_get_last_error`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - The result comes from process-global PCNTL bridge state.

eval_builtin! { contract: "pcntl_get_last_error", area: Pcntl, direct: Pcntl, values: Pcntl }
