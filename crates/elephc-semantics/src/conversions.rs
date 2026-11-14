//! Purpose:
//! Selects scalar conversion rules by semantic site rather than backend implementation.
//!
//! Called from:
//! - Checker, lowering, and eval adapters after resolving full language types.
//!
//! Key details:
//! - Formatting and storage adaptation are explicit contexts, separate from value coercion.
//! - Nominal classes and full union assignability remain owned by caller type systems.

use crate::{SemanticProfile, StrictFeature};

/// Scalar categories sufficient to decide a value-conversion policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind { Integer, Float, Boolean, String, Null, Dynamic, NonScalar }

/// The language operation requesting conversion, independent of physical storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversionSite {
    ImplicitValue, BooleanCondition, Concatenation, ArrayKey, ExplicitCast,
    OutputFormatting, StorageAdaptation,
}

/// Runtime kinds permitted by a checked conversion boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllowedKinds(u8);

impl AllowedKinds {
    /// Starts an explicit contract with no permitted scalar kinds.
    pub const EMPTY: Self = Self(0);

    /// Returns the set containing one scalar kind, or empty for unresolved/non-scalar kinds.
    pub const fn one(kind: ValueKind) -> Self {
        Self(match kind {
            ValueKind::Integer => 1, ValueKind::Float => 2, ValueKind::Boolean => 4,
            ValueKind::String => 8, ValueKind::Null => 16,
            ValueKind::Dynamic | ValueKind::NonScalar => 0,
        })
    }

    /// Combines the allowed kinds of two independently accepted alternatives.
    pub const fn union(self, other: Self) -> Self { Self(self.0 | other.0) }

    /// Tests a concrete scalar category against this checked boundary.
    pub const fn allows(self, kind: ValueKind) -> bool { self.0 & Self::one(kind).0 != 0 }
}

/// Plans exact scalar membership for a resolved declaration, including scalar unions.
pub fn plan_implicit_kinds(profile: SemanticProfile, source: ValueKind, allowed: AllowedKinds)
    -> ConversionPlan {
    if !profile.contains(StrictFeature::Coercions) { return ConversionPlan::ExistingBehavior; }
    if source == ValueKind::Dynamic {
        return ConversionPlan::Guard { feature: StrictFeature::Coercions, allowed };
    }
    if allowed.allows(source) { ConversionPlan::Identity }
    else { ConversionPlan::Reject(StrictFeature::Coercions) }
}

/// Semantic decision consumed by both static and runtime conversion adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversionPlan {
    /// Retain the existing PHP/full-type behavior at this site.
    ExistingBehavior,
    /// Preserve the value without a PHP-visible conversion.
    Identity,
    /// Refuse a statically known incompatible kind under this guarantee.
    Reject(StrictFeature),
    /// Check the dynamic tag without coercing the value.
    Guard { feature: StrictFeature, allowed: AllowedKinds },
    /// Use a domain/range-checked explicit conversion.
    CheckedCast(ValueKind),
}

/// Plans a scalar conversion under the independently selected guarantee owning its site.
pub fn plan_conversion(profile: SemanticProfile, site: ConversionSite,
    source: ValueKind, target: ValueKind) -> ConversionPlan {
    use ConversionPlan::*;
    use ConversionSite::*;
    let (feature, allowed) = match site {
        StorageAdaptation | OutputFormatting => return ExistingBehavior,
        ExplicitCast => return if profile.contains(StrictFeature::Casts) {
            CheckedCast(target)
        } else { ExistingBehavior },
        BooleanCondition => (StrictFeature::Bools, AllowedKinds::one(ValueKind::Boolean)),
        Concatenation => (StrictFeature::Strings, AllowedKinds::one(ValueKind::String)),
        ArrayKey => (StrictFeature::ArrayKeys,
            AllowedKinds::one(ValueKind::Integer).union(AllowedKinds::one(ValueKind::String))),
        ImplicitValue => {
            if matches!(target, ValueKind::Dynamic | ValueKind::NonScalar) { return ExistingBehavior; }
            return plan_implicit_kinds(profile, source, AllowedKinds::one(target));
        }
    };
    if !profile.contains(feature) { return ExistingBehavior; }
    if source == ValueKind::Dynamic { return Guard { feature, allowed }; }
    if allowed.allows(source) { Identity } else { Reject(feature) }
}
