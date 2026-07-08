//! Purpose:
//! Shared helper for the stat/lstat/fstat builtin homes in the io area.
//! Provides the `stat_result_type` helper that returns the normalized PHP return
//! type for stat-family functions (`assoc-array<mixed, int>|bool`).
//!
//! Called from:
//! - `crate::builtins::io::stat` (check hook)
//! - `crate::builtins::io::lstat` (check hook)
//! - `crate::builtins::io::fstat` (check hook)
//!
//! Key details:
//! - The union type reflects PHP's stat functions returning `array|false`: the
//!   AssocArray represents the stat buffer (mode, ino, uid, etc. as int values),
//!   and Bool represents the `false` return on failure.
//! - `Mixed` is used as the key type to reflect PHP's heterogeneous array indexing
//!   (stat arrays are accessible by both numeric and string keys).

use crate::types::checker::Checker;
use crate::types::PhpType;

/// Returns the normalized return type for `stat()` / `lstat()` / `fstat()`.
///
/// Produces `assoc-array<mixed, int>|bool` as a normalized union type. PHP's stat functions
/// return `array|false` — the AssocArray represents the stat buffer keys (mode, ino, uid, etc.
/// as int values), and `Bool` represents the false return on failure. The `Mixed` key type
/// reflects PHP's heterogeneous array indexing (stat arrays are accessible by both numeric
/// and string keys).
pub(crate) fn stat_result_type(checker: &Checker) -> PhpType {
    checker.normalize_union_type(vec![
        PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Int),
        },
        PhpType::Bool,
    ])
}
