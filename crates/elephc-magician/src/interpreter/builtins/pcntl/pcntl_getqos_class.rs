//! Purpose:
//! Declares the macOS Magician binding for `pcntl_getqos_class`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - The bridge ordinal maps back to the generated `Pcntl\QosClass` singleton.

eval_builtin! { contract: "pcntl_getqos_class", area: Pcntl, direct: Pcntl, values: Pcntl }
