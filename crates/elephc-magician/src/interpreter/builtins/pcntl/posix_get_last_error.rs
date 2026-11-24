//! Purpose:
//! Declares the Magician binding for `posix_get_last_error`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Eval reads the same ext/posix last-error state as compiled code.

eval_builtin! { contract: "posix_get_last_error", area: Pcntl, direct: Pcntl, values: Pcntl }
