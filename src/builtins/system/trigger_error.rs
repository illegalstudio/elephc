//! Purpose:
//! Registers PHP `trigger_error()` through the typed Core EIR surface.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - Source path and line are preserved for user handler arguments and default diagnostics.

use super::core_support::core_builtin_home;

core_builtin_home!("trigger_error", TriggerError);
