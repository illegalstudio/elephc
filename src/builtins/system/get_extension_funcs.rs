//! Purpose:
//! Registers PHP `get_extension_funcs()` through the typed Core EIR surface.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - Extension membership derives from the same catalog and linked-feature metadata as availability.

use super::core_support::core_builtin_home;

core_builtin_home!(
    "get_extension_funcs",
    GetExtensionFuncs,
    check_boxed: super::core_support::check_extension_functions
);
