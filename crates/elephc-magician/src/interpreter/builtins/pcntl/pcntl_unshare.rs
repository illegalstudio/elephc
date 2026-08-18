//! Purpose:
//! Declares the Linux Magician binding for `pcntl_unshare`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Namespace flags are forwarded through the stable bridge.

eval_builtin! { contract: "pcntl_unshare", area: Pcntl, direct: Pcntl, values: Pcntl }
