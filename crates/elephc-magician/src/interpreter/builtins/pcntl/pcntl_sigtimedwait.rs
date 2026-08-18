//! Purpose:
//! Declares the Linux Magician binding for `pcntl_sigtimedwait`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Signal information is written only when the bridge reports delivery.

eval_builtin! { contract: "pcntl_sigtimedwait", area: Pcntl, direct: Pcntl, values: Pcntl }
