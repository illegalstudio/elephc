//! Purpose:
//! Registers PHP `set_exception_handler()` through the typed Core EIR surface.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - Lowering retains the original PHP callback value and a normalized invocation descriptor.

use super::core_support::core_builtin_home;

core_builtin_home!("set_exception_handler", SetExceptionHandler);
