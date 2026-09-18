//! Purpose:
//! Registers PHP `debug_backtrace()` through the typed Core EIR surface.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - Runtime lowering owns frame selection, argument inclusion, and limit handling.

use super::core_support::core_builtin_home;

core_builtin_home!(
    "debug_backtrace",
    DebugBacktrace,
    check: super::core_support::check_mixed_array
);
