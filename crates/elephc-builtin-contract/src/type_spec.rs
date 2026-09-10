//! Purpose:
//! Renders and validates backend-neutral PHP builtin type declarations.
//!
//! Called from:
//! - Contract registry validation and generated documentation exporters.
//!
//! Key details:
//! - Union alternatives retain false and null as distinct singleton types.
//! - Invalid or redundant declarations fail before backend bindings consume them.

use std::fmt;
use crate::TypeSpec;

impl fmt::Display for TypeSpec {
    /// Writes the PHP type spelling, preserving explicit union alternatives and nullable shorthand.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Int => "int",
            Self::Float => "float",
            Self::Str => "string",
            Self::Bool => "bool",
            Self::False => "false",
            Self::Null => "null",
            Self::Mixed => "mixed",
            Self::Array => "array",
            Self::Void => "void",
            Self::Ptr => "pointer",
            Self::Callable => "callable",
            Self::Nullable(inner) => return write!(formatter, "?{inner}"),
            Self::Union(members) => {
                for (index, member) in members.iter().enumerate() {
                    if index != 0 { formatter.write_str("|")?; }
                    write!(formatter, "{member}")?;
                }
                return Ok(());
            }
        };
        formatter.write_str(name)
    }
}

impl TypeSpec {
    /// Rejects malformed unions, duplicate alternatives, and invalid nullable declarations.
    pub fn validate(self) -> Result<(), &'static str> {
        match self {
            Self::Union(members) => {
                if members.len() < 2 { return Err("a union needs at least two alternatives"); }
                for (index, member) in members.iter().enumerate() {
                    if matches!(member, Self::Mixed | Self::Void | Self::Union(_) | Self::Nullable(_)) {
                        return Err("union alternatives must be atomic value types");
                    }
                    if members[..index].contains(member) { return Err("duplicate union alternative"); }
                    if *member == Self::False && members.contains(&Self::Bool) {
                        return Err("false is redundant alongside bool");
                    }
                }
                Ok(())
            }
            Self::Nullable(inner) => {
                if matches!(inner, Self::Mixed | Self::Void | Self::Null | Self::Union(_) | Self::Nullable(_)) {
                    return Err("nullable shorthand needs one non-null value type");
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the signatures needed by scalar, container, and nullable mbstring operations.
    #[test]
    fn php_union_type_spellings() {
        for (ty, expected) in [
            (TypeSpec::Union(&[TypeSpec::Int, TypeSpec::False]), "int|false"),
            (TypeSpec::Union(&[TypeSpec::Str, TypeSpec::Bool]), "string|bool"),
            (TypeSpec::Union(&[TypeSpec::Array, TypeSpec::Str, TypeSpec::Null]), "array|string|null"),
            (TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False, TypeSpec::Null]), "string|false|null"),
            (TypeSpec::Nullable(&TypeSpec::Int), "?int"),
        ] {
            assert_eq!(ty.to_string(), expected);
            assert_eq!(ty.validate(), Ok(()));
        }
    }

    /// Verifies invalid declarations cannot hide missing alternatives or contradictory metadata.
    #[test]
    fn rejects_invalid_php_union_types() {
        for ty in [
            TypeSpec::Union(&[]), TypeSpec::Union(&[TypeSpec::Str]),
            TypeSpec::Union(&[TypeSpec::Str, TypeSpec::Str]),
            TypeSpec::Union(&[TypeSpec::Mixed, TypeSpec::Int]),
            TypeSpec::Union(&[TypeSpec::Void, TypeSpec::Int]),
            TypeSpec::Union(&[TypeSpec::Bool, TypeSpec::False]),
            TypeSpec::Union(&[TypeSpec::Nullable(&TypeSpec::Str), TypeSpec::Int]),
            TypeSpec::Nullable(&TypeSpec::Union(&[TypeSpec::Str, TypeSpec::Int])),
        ] { assert!(ty.validate().is_err(), "{ty:?}"); }
    }
}
