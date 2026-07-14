//! Purpose:
//! Defines class-alias kinds and synthetic ReflectionAttribute metadata.
//!
//! Called from:
//! - Class alias registration and ReflectionAttribute construction.
//!
//! Key details:
//! - Alias kind prevents cross-kind lookup while attribute target/repetition stays attached to identity.

use super::*;

/// PHP class-like declaration kind targeted by a dynamic `class_alias()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EvalClassAliasKind {
    Class,
    Interface,
    Trait,
    Enum,
}

/// Dynamic alias target and kind recorded for eval-visible class-like symbols.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EvalClassAlias {
    pub(super) target: String,
    pub(super) kind: EvalClassAliasKind,
}

/// Metadata attached to one synthetic eval `ReflectionAttribute` object.
#[derive(Clone)]
pub struct EvalReflectionAttributeMetadata {
    pub(super) attribute: EvalAttribute,
    pub(super) target: u64,
    pub(super) repeated: bool,
}

impl EvalReflectionAttributeMetadata {
    /// Creates metadata for a materialized `ReflectionAttribute` object.
    pub fn new(attribute: EvalAttribute, target: u64, repeated: bool) -> Self {
        Self {
            attribute,
            target,
            repeated,
        }
    }

    /// Returns the underlying eval-retained attribute metadata.
    pub const fn attribute(&self) -> &EvalAttribute {
        &self.attribute
    }

    /// Returns the PHP `Attribute::TARGET_*` bitmask for this reflected owner.
    pub const fn target(&self) -> u64 {
        self.target
    }

    /// Returns whether this owner has multiple attributes with the same name.
    pub const fn is_repeated(&self) -> bool {
        self.repeated
    }
}
