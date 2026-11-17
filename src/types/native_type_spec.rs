//! Purpose:
//! Formats checked PHP value contracts for native metadata and eval registration.
//!
//! Called from:
//! - Callable signatures, local reference descriptors and scope producer publication.
//!
//! Key details:
//! - Null value contracts are distinct from void return declarations.
//! - All consumers share the existing native callable type grammar.

use super::PhpType;

/// Formats one checked PHP type for eval native metadata registration.
pub(crate) fn php_type_spec(php_type: &PhpType, allow_return_atoms: bool) -> Option<String> {
    if php_type.is_php_array() {
        return Some("array".to_string());
    }
    match php_type {
        PhpType::Int => Some("int".to_string()),
        PhpType::Float => Some("float".to_string()),
        PhpType::Str => Some("string".to_string()),
        PhpType::Bool => Some("bool".to_string()),
        PhpType::False => Some("false".to_string()),
        PhpType::Void if allow_return_atoms => Some("void".to_string()),
        PhpType::Void => Some("null".to_string()),
        PhpType::Never if allow_return_atoms => Some("never".to_string()),
        PhpType::Never => None,
        PhpType::Iterable => Some("iterable".to_string()),
        PhpType::Mixed => Some("mixed".to_string()),
        PhpType::Array(_) | PhpType::AssocArray { .. } => Some("array".to_string()),
        PhpType::Callable => Some("callable".to_string()),
        PhpType::Object(name) if name.is_empty() => Some("object".to_string()),
        PhpType::Object(name) => Some(name.clone()),
        PhpType::Union(members) => member_specs(members),
        PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_)
        | PhpType::Resource(_)
        | PhpType::Narrow(_)
        | PhpType::TaggedScalar => None,
    }
}

/// Formats union members from checked PHP types for eval native metadata registration.
pub(crate) fn member_specs(members: &[PhpType]) -> Option<String> {
    members
        .iter()
        .map(|member| php_type_spec(member, false))
        .collect::<Option<Vec<_>>>()
        .map(|members| members.join("|"))
}

