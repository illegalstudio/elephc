//! Purpose:
//! Declares the Magician binding for `pcntl_getpriority`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - The bridge separates failure from the valid priority value `-1`.

eval_builtin! { contract: "pcntl_getpriority", area: Pcntl, direct: Pcntl, values: Pcntl }
