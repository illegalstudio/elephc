//! Purpose:
//! Declares the Magician binding for `pcntl_signal_get_handler`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Returned callable cells receive an independent retained owner.

eval_builtin! { contract: "pcntl_signal_get_handler", area: Pcntl, direct: Pcntl, values: Pcntl }
