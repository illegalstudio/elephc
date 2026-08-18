//! Purpose:
//! Declares the Magician binding for `pcntl_sigprocmask`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Direct calls preserve optional old-mask writeback storage.

eval_builtin! { contract: "pcntl_sigprocmask", area: Pcntl, direct: Pcntl, values: Pcntl }
