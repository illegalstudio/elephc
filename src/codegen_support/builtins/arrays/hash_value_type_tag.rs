//! Purpose:
//! Maps PHP element types to runtime hash-array value tags.
//! Provides the compact tag contract used when building associative array payloads.
//!
//! Called from:
//! - `crate::codegen_support::builtins::arrays::{array_combine,array_fill_keys}::emit()`.
//!
//! Key details:
//! - Tag values must stay synchronized with runtime hash helpers that interpret Mixed and typed payloads.

use crate::types::PhpType;

/// Maps a `PhpType` to its corresponding runtime hash-array value tag.
///
/// The returned tag is embedded in hash table payloads to identify the type of
/// each stored value. Tags `0`–`10` map to specific PHP types; tag `7` is used
/// as a fallback for `Mixed`, `Union`, and `Iterable` since they can hold any type.
///
/// # Arguments
/// * `ty` — the PHP type to map to a tag
///
/// # Returns
/// A `u8` tag value in range `0..=10` identifying the value type for hash storage.
pub(super) fn hash_value_type_tag(ty: &PhpType) -> u8 {
    match ty {
        PhpType::Int => 0,
        PhpType::Str => 1,
        PhpType::Float => 2,
        PhpType::Bool => 3,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed => 7,
        PhpType::Union(_) => 7,
        PhpType::Iterable => 7,
        PhpType::Void => 8,
        PhpType::Resource(_) => 9,
        PhpType::Callable => 10,
        PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) | PhpType::Never => 0,
        PhpType::TaggedScalar => {
            unreachable!("TaggedScalar must be narrowed or boxed before hash storage")
        }
    }
}
