//! Purpose:
//! Registers PHP `get_exception_handler()` through the typed Core EIR surface.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - The result is an independently retained copy of the active callback value, or PHP null.
//! - The internal invocation descriptor owner is never exposed to PHP code.

use super::core_support::core_builtin_home;

core_builtin_home!("get_exception_handler", GetExceptionHandler);
