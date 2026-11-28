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
    /// Supplies the ordinary PHP message when a prepared callable runs without checked casts.
    pub const fn php_message(self) -> &'static str {
        match self {
            Self::Invalid => "settype(): Argument #2 ($type) must be a valid type",
            Self::Resource => "Cannot convert to resource type",
        }
    }

    /// Keeps original-source diagnostics and checked runtime target errors consistent.
    pub const fn message(self) -> &'static str {
        match self {
            Self::Invalid => "--strict-casts: settype(): Argument #2 ($type) must be a valid type",
            Self::Resource => "--strict-casts: Cannot convert to resource type",
        }
    }
}

impl SettypeTarget {
    /// Enumerates valid targets independently of backend conversion support.
    pub const ALL: [Self; 7] = [Self::Bool, Self::Int, Self::Float, Self::String, Self::Array, Self::Object, Self::Null];

    /// Supplies canonical names and aliases to both static and runtime target planners.
    pub const fn names(self) -> &'static [&'static str] {
        match self {
            Self::Bool => &["bool", "boolean"], Self::Int => &["int", "integer"],
            Self::Float => &["float", "double"], Self::String => &["string"],
            Self::Array => &["array"], Self::Object => &["object"], Self::Null => &["null"],
        }
    }

    /// Resolves complete target names without trimming or accepting trailing bytes.
    pub fn parse(name: &str) -> Result<Self, SettypeTargetFailure> {
        if name.eq_ignore_ascii_case("resource") { return Err(SettypeTargetFailure::Resource); }
        Self::ALL.into_iter().find(|target| target.names().iter().any(|alias| name.eq_ignore_ascii_case(alias)))
            .ok_or(SettypeTargetFailure::Invalid)
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
