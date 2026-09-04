//! Purpose:
//! Registers PHP `debug_print_backtrace()` through the typed Core EIR surface.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - Runtime lowering prints the selected frame sequence and returns void.

use super::core_support::core_builtin_home;

core_builtin_home!("debug_print_backtrace", DebugPrintBacktrace);
