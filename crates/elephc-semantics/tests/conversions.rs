//! Purpose:
//! Tests independent ownership of scalar conversion sites and dynamic guards.
//!
//! Called from:
//! - Cargo's elephc-semantics conversion integration-test binary.
//!
//! Key details:
//! - Explicit formatting and representation changes do not become implicit value casts.

use elephc_semantics::{SemanticProfile, StrictFeature};
use elephc_semantics::conversions::{plan_conversion, AllowedKinds, ConversionPlan, ConversionSite, ValueKind};

/// Strict scalar binding rejects numeric widening as well as unrelated scalar coercions.
#[test]
fn strict_conversion_implicit_value_requires_an_exact_kind() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    assert_eq!(plan_conversion(profile, ConversionSite::ImplicitValue, ValueKind::Integer, ValueKind::Float),
        ConversionPlan::Reject(StrictFeature::Coercions));
    assert_eq!(plan_conversion(profile, ConversionSite::ImplicitValue, ValueKind::Float, ValueKind::Float), ConversionPlan::Identity);
    assert_eq!(plan_conversion(profile, ConversionSite::ImplicitValue, ValueKind::Dynamic, ValueKind::Integer),
        ConversionPlan::Guard { feature: StrictFeature::Coercions, allowed: AllowedKinds::one(ValueKind::Integer) });
}

/// Conditions, concatenation, and key conversion remain independently selectable.
#[test]
fn strict_conversion_sites_select_their_own_guarantee() {
    for (site, feature, target) in [
        (ConversionSite::BooleanCondition, StrictFeature::Bools, ValueKind::Boolean),
        (ConversionSite::Concatenation, StrictFeature::Strings, ValueKind::String),
        (ConversionSite::ArrayKey, StrictFeature::ArrayKeys, ValueKind::String),
    ] {
        let profile = SemanticProfile::PERMISSIVE.with(feature, true);
        assert_eq!(plan_conversion(profile, site, ValueKind::Null, target), ConversionPlan::Reject(feature));
        assert_eq!(plan_conversion(SemanticProfile::all().with(feature, false), site, ValueKind::Null, target), ConversionPlan::ExistingBehavior);
    }
}

/// Checked casts and output/storage contexts retain their explicit semantic purpose.
#[test]
fn strict_conversion_explicit_contexts_do_not_become_implicit_coercions() {
    for site in [ConversionSite::OutputFormatting, ConversionSite::StorageAdaptation] {
        assert_eq!(plan_conversion(SemanticProfile::all(), site, ValueKind::Integer, ValueKind::String), ConversionPlan::ExistingBehavior);
    }
    assert_eq!(plan_conversion(SemanticProfile::all(), ConversionSite::ExplicitCast, ValueKind::Float, ValueKind::Integer),
        ConversionPlan::CheckedCast(ValueKind::Integer));
    let keys = AllowedKinds::one(ValueKind::Integer).union(AllowedKinds::one(ValueKind::String));
    assert!(keys.allows(ValueKind::Integer));
    assert!(keys.allows(ValueKind::String));
    assert!(!keys.allows(ValueKind::Float));
    assert!(!keys.allows(ValueKind::Null));
}
