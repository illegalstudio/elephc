//! Purpose:
//! Registers PHP `user_error()` as the typed alias of `trigger_error()`.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - The alias shares handler dispatch, validation, diagnostics, and return semantics.

use super::core_support::core_builtin_home;

core_builtin_home!("user_error", TriggerError);
