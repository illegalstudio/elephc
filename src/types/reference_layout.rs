//! Purpose:
//! Classifies property contracts whose managed cells can acquire canonical boxed payloads.
//!
//! Called from:
//! - Checker promotion, reference argument selection and target/runtime layout adapters.
//!
//! Key details:
//! - Logical nullable and union contracts remain distinct from their physical payload layout.
//! - Raw iterable containers, pointers and borrowed storage require separate protocols.

use super::PhpType;

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
        _ => return None,
    })
}

/// Keeps unsupported declaration members out of the canonical reference protocol.
fn declaration_contract(ty: &PhpType) -> bool {
    match ty {
        PhpType::Mixed | PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False
        | PhpType::Str | PhpType::Void | PhpType::Object(_) | PhpType::Array(_)
        | PhpType::AssocArray { .. } => true,
        PhpType::Union(members) => !members.is_empty() && members.iter().all(declaration_contract),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for ty in [PhpType::Iterable, PhpType::Pointer(None)] {
            assert_eq!(canonical_property_reference_payload(&ty), None, "{ty:?}");
        }
    }
}
