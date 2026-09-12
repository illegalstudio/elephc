//! Purpose:
//! Registers PHP `get_defined_functions()` through the typed Core EIR surface.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - Internal and user function inventories come from shared compiler metadata.

use super::core_support::core_builtin_home;

core_builtin_home!(
    "get_defined_functions",
    GetDefinedFunctions,
    check: super::core_support::check_defined_functions
);
