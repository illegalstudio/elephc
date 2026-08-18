//! Purpose:
//! Declares the Magician binding for `pcntl_exec`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Arguments and environment are copied into the shared bridge before `execve`.

eval_builtin! { contract: "pcntl_exec", area: Pcntl, direct: Pcntl, values: Pcntl }
