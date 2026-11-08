//! Purpose:
//! Declares the Magician binding for `posix_errno`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - This alias reads the shared ext/posix last-error state.

eval_builtin! { contract: "posix_errno", area: Pcntl, direct: Pcntl, values: Pcntl }
