//! Purpose:
//! Declares the macOS Magician binding for `pcntl_setqos_class`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Omission materializes `Pcntl\QosClass::Default` through the shared contract.

eval_builtin! { contract: "pcntl_setqos_class", area: Pcntl, direct: Pcntl, values: Pcntl }
