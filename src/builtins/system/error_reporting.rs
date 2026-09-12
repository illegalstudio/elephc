//! Purpose:
//! Registers PHP `error_reporting()` through the typed Core EIR surface.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - The runtime operation atomically returns the previous mask when setting a new one.

use super::core_support::core_builtin_home;

core_builtin_home!("error_reporting", ErrorReporting);
