//! Purpose:
//! Registers PHP `restore_exception_handler()` through the typed Core EIR surface.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - Nested exception handler state is restored in last-in, first-out order.

use super::core_support::core_builtin_home;

core_builtin_home!("restore_exception_handler", RestoreExceptionHandler);
