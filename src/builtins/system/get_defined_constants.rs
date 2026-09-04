//! Purpose:
//! Registers PHP `get_defined_constants()` through the typed Core EIR surface.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - Backend metadata supplies built-in and user-defined constants without PHP-name dispatch.

use super::core_support::core_builtin_home;

core_builtin_home!(
    "get_defined_constants",
    GetDefinedConstants,
    check: super::core_support::check_string_mixed_hash
);
