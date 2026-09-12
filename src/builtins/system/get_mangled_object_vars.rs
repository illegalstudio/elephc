//! Purpose:
//! Registers PHP `get_mangled_object_vars()` through the typed Core EIR surface.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - Projection keeps PHP visibility-mangled keys and ignores the caller's lexical visibility.

use super::core_support::core_builtin_home;

core_builtin_home!(
    "get_mangled_object_vars",
    GetMangledObjectVars,
    check: super::core_support::check_string_mixed_hash
);
