//! Purpose:
//! Registers PHP `get_required_files()` as the typed include-inventory alias.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - The alias shares ordering and deduplication with `get_included_files()`.

use super::core_support::core_builtin_home;

core_builtin_home!(
    "get_required_files",
    GetIncludedFiles,
    check: super::core_support::check_string_array
);
