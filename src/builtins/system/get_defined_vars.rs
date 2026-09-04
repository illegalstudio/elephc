//! Purpose:
//! Registers PHP `get_defined_vars()` through the typed Core EIR surface.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - Backend lowering projects visible PHP locals from the active function frame.

use super::core_support::core_builtin_home;

core_builtin_home!(
    "get_defined_vars",
    GetDefinedVars,
    check: super::core_support::check_string_mixed_hash,
    no_first_class: "Cannot call get_defined_vars() dynamically"
);
