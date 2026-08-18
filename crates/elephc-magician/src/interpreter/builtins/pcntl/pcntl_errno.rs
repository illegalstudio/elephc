//! Purpose:
//! Declares the Magician binding for `pcntl_errno`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - This alias reads the shared PCNTL last-error state.

eval_builtin! { contract: "pcntl_errno", area: Pcntl, direct: Pcntl, values: Pcntl }
