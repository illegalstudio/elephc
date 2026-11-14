//! Purpose:
//! Classifies PHP settype targets once for checking, lowering and eval.
//!
//! Called from:
//! - Explicit numeric cast aliases, original-source validation and settype dispatch.
//!
//! Key details:
//! - Valid target classification is separate from each backend's implemented conversions.
//! - Resource conversion has PHP's distinct invalid-target failure.

use crate::numeric_cast::NumericCastTarget;

/// PHP-visible conversion target after resolving case-insensitive aliases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettypeTarget { Bool, Int, Float, String, Array, Object, Null }

/// Invalid target name or forbidden resource conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettypeTargetFailure { Invalid, Resource }

impl SettypeTargetFailure {
    /// Keeps original-source diagnostics and checked runtime target errors consistent.
    pub const fn message(self) -> &'static str {
        match self {
            Self::Invalid => "--strict-casts: settype(): Argument #2 ($type) must be a valid type",
            Self::Resource => "--strict-casts: Cannot convert to resource type",
        }
    }
}

impl SettypeTarget {
    /// Resolves complete target names without trimming or accepting trailing bytes.
    pub fn parse(name: &str) -> Result<Self, SettypeTargetFailure> {
        if name.eq_ignore_ascii_case("bool") || name.eq_ignore_ascii_case("boolean") { Ok(Self::Bool) }
        else if name.eq_ignore_ascii_case("int") || name.eq_ignore_ascii_case("integer") { Ok(Self::Int) }
        else if name.eq_ignore_ascii_case("float") || name.eq_ignore_ascii_case("double") { Ok(Self::Float) }
        else if name.eq_ignore_ascii_case("string") { Ok(Self::String) }
        else if name.eq_ignore_ascii_case("array") { Ok(Self::Array) }
        else if name.eq_ignore_ascii_case("object") { Ok(Self::Object) }
        else if name.eq_ignore_ascii_case("null") { Ok(Self::Null) }
        else if name.eq_ignore_ascii_case("resource") { Err(SettypeTargetFailure::Resource) }
        else { Err(SettypeTargetFailure::Invalid) }
    }

    /// Selects numeric conversion without conflating other valid settype targets with bad names.
    pub const fn numeric_target(self) -> Option<NumericCastTarget> {
        match self { Self::Int => Some(NumericCastTarget::Int), Self::Float => Some(NumericCastTarget::Float), _ => None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every scalar alias and valid nonnumeric target shares one complete-name classifier.
    #[test]
    fn strict_settype_target_aliases_and_valid_domains() {
        for (name, target) in [("BOOLEAN", SettypeTarget::Bool), ("int", SettypeTarget::Int),
            ("InTeGeR", SettypeTarget::Int), ("DOUBLE", SettypeTarget::Float),
            ("float", SettypeTarget::Float), ("string", SettypeTarget::String),
            ("array", SettypeTarget::Array), ("object", SettypeTarget::Object), ("NULL", SettypeTarget::Null)] {
            assert_eq!(SettypeTarget::parse(name), Ok(target), "{name}");
        }
        assert_eq!(SettypeTarget::Array.numeric_target(), None);
        assert_eq!(SettypeTarget::Int.numeric_target(), Some(NumericCastTarget::Int));
    }

    /// Invalid names, NUL and trailing data cannot silently report a successful conversion.
    #[test]
    fn strict_settype_target_invalid_names_and_resource_failure() {
        for name in ["", "unset", "mixed", "int\0x", "int ", " int", "integer[]"] {
            assert_eq!(SettypeTarget::parse(name), Err(SettypeTargetFailure::Invalid), "{name:?}");
        }
        assert_eq!(SettypeTarget::parse("RESOURCE"), Err(SettypeTargetFailure::Resource));
    }
}
