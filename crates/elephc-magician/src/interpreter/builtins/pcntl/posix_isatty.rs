//! Purpose:
//! Declares the Magician binding for `posix_isatty`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Eval accepts an integer descriptor or a stream resource, as PHP does.

eval_builtin! { contract: "posix_isatty", area: Pcntl, direct: Pcntl, values: Pcntl }
