//! Purpose:
//! Classifies property contracts whose managed cells can acquire canonical boxed payloads.
//!
//! Called from:
//! - Checker promotion, reference argument selection and target/runtime layout adapters.
//!
//! Key details:
//! - Logical nullable and union contracts remain distinct from their physical payload layout.
//! - Iterable payloads preserve their concrete container kind; pointers and borrowed storage stay separate.

use super::{ClassInfo, PhpType};

/// Selects managed declaration storage for independent checked conversion guarantees.
pub(crate) fn checked_property_references(profile: elephc_semantics::SemanticProfile) -> bool {
    profile.contains(elephc_semantics::StrictFeature::Casts)
        || profile.contains(elephc_semantics::StrictFeature::Coercions)
}

/// Keeps declaration types intact while selecting boxed storage for checked static references.
pub(crate) fn static_property_storage_type(
    class: &ClassInfo, property: &str, profile: elephc_semantics::SemanticProfile,
) -> Option<PhpType> {
    let (_, ty) = class.static_properties.iter().find(|(name, _)| name == property)?;
    Some(if checked_property_references(profile) && !class.is_internal
        && class.declared_static_properties.contains(property)
        && canonical_property_reference_payload(ty).is_some()
    { PhpType::Mixed } else { ty.clone() })
}

/// The original physical payload that a managed property reference must preserve during promotion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CanonicalReferencePayload {
    Boxed,
    Int,
    Float,
    Bool,
    String,
    NullableInt,
    Object,
    Array,
    Iterable,
}

/// Selects reviewed declaration layouts without inferring indirection from pointer size.
pub(crate) fn canonical_property_reference_payload(ty: &PhpType) -> Option<CanonicalReferencePayload> {
    if !declaration_contract(ty) { return None; }
    Some(match ty.codegen_repr() {
        PhpType::Mixed => CanonicalReferencePayload::Boxed,
        PhpType::Int => CanonicalReferencePayload::Int,
        PhpType::Float => CanonicalReferencePayload::Float,
        PhpType::Bool => CanonicalReferencePayload::Bool,
        PhpType::Str => CanonicalReferencePayload::String,
        PhpType::TaggedScalar => CanonicalReferencePayload::NullableInt,
        PhpType::Object(_) => CanonicalReferencePayload::Object,
        PhpType::Array(_) | PhpType::AssocArray { .. } => CanonicalReferencePayload::Array,
        PhpType::Iterable => CanonicalReferencePayload::Iterable,
        _ => return None,
    })
}

/// Keeps unsupported declaration members out of the canonical reference protocol.
fn declaration_contract(ty: &PhpType) -> bool {
    match ty {
        PhpType::Mixed | PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False
        | PhpType::Str | PhpType::Void | PhpType::Object(_) | PhpType::Array(_)
        | PhpType::AssocArray { .. } | PhpType::Iterable => true,
        PhpType::Union(members) => !members.is_empty() && members.iter().all(declaration_contract),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Physical checked storage changes without changing the class's original declaration metadata.
    #[test]
    fn checked_static_storage_keeps_declaration_contracts() {
        let tokens = crate::lexer::tokenize("<?php class StaticLayouts { public static int $number = 1; \
            public static float $fraction = 1.5; public static string $text = 'x'; \
            public static bool $ready = true; public static ?int $optional = null; }").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let checked = crate::types::check(&program).unwrap();
        let class = &checked.classes["StaticLayouts"];
        let profile = elephc_semantics::SemanticProfile::PERMISSIVE;
        for (name, original) in &class.static_properties {
            assert_eq!(static_property_storage_type(class, name, profile), Some(original.clone()));
            for feature in [elephc_semantics::StrictFeature::Casts, elephc_semantics::StrictFeature::Coercions] {
                assert_eq!(static_property_storage_type(class, name,
                    profile.with(feature, true)), Some(PhpType::Mixed));
            }
        }
        assert_eq!(class.static_properties[0].1, PhpType::Int);
    }

    /// Nullable declarations select their actual ABI without admitting unrelated pointer layouts.
    #[test]
    fn canonical_property_reference_payload_preserves_nullable_layouts() {
        assert_eq!(canonical_property_reference_payload(&PhpType::Union(vec![PhpType::Int, PhpType::Void])),
            Some(CanonicalReferencePayload::NullableInt));
        for members in [vec![PhpType::Str, PhpType::Void], vec![PhpType::Int, PhpType::Str],
            vec![PhpType::Bool, PhpType::Void], vec![PhpType::Float, PhpType::Str],
            vec![PhpType::Object("NativeObject".into()), PhpType::Void]] {
            assert_eq!(canonical_property_reference_payload(&PhpType::Union(members)),
                Some(CanonicalReferencePayload::Boxed));
        }
        assert_eq!(canonical_property_reference_payload(&PhpType::Object("NativeObject".into())),
            Some(CanonicalReferencePayload::Object));
        assert_eq!(canonical_property_reference_payload(&PhpType::Array(Box::new(PhpType::Int))),
            Some(CanonicalReferencePayload::Array));
        assert_eq!(canonical_property_reference_payload(&PhpType::Iterable),
            Some(CanonicalReferencePayload::Iterable));
        assert_eq!(canonical_property_reference_payload(&PhpType::Pointer(None)), None);
    }
}
