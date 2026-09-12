//! Purpose:
//! Registers PHP `restore_error_handler()` through the typed Core EIR surface.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - Nested handler state is restored in last-in, first-out order.

use super::core_support::core_builtin_home;

core_builtin_home!("restore_error_handler", RestoreErrorHandler);
