//! Purpose:
//! Declares the Magician binding for `posix_getpgrp`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Eval returns the bridge's widened process group id.

eval_builtin! { contract: "posix_getpgrp", area: Pcntl, direct: Pcntl, values: Pcntl }
