//! Purpose:
//! Registers PHP `get_included_files()` through the typed Core EIR surface.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - `get_required_files()` shares this operation because PHP defines it as an alias.

use super::core_support::core_builtin_home;

core_builtin_home!(
    "get_included_files",
    GetIncludedFiles,
    check: super::core_support::check_string_array
);
