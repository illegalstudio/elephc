//! Purpose:
//! Declares the Magician binding for `pcntl_waitpid`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Selected-child waits share status and rusage conversion with `pcntl_wait`.

eval_builtin! { contract: "pcntl_waitpid", area: Pcntl, direct: Pcntl, values: Pcntl }
