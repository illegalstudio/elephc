//! Purpose:
//! Registers PHP `get_resources()` through the typed Core EIR surface.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - Runtime lowering reconstructs active resource values from the shared resource registry.

use super::core_support::core_builtin_home;

core_builtin_home!(
    "get_resources",
    GetResources,
    check: super::core_support::check_resource_hash
);
