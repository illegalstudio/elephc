//! Purpose:
//! Groups miscellaneous filesystem operation eval builtins by operation family.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem` re-exports.
//!
//! Key details:
//! - Helpers return PHP-compatible false/null/string/int cells via `RuntimeValueOps`
//!   while path coercion remains shared with sibling filesystem modules.

mod disk;
mod glob;
mod links;
mod listing;
mod path_bool;
mod tempnam;
mod touch;
mod umask;

pub(in crate::interpreter) use disk::*;
pub(in crate::interpreter) use glob::*;
pub(in crate::interpreter) use links::*;
pub(in crate::interpreter) use listing::*;
pub(in crate::interpreter) use path_bool::*;
pub(in crate::interpreter) use tempnam::*;
pub(in crate::interpreter) use touch::*;
pub(in crate::interpreter) use umask::*;
