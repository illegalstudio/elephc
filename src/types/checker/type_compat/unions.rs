//! Purpose:
//! Checks type compatibility for unions cases.
//! Supports the central assignability predicate used by declarations, calls, returns, and assignments.
//!
//! Called from:
//! - `crate::types::checker::type_compat`
//!
//! Key details:
//! - Rules here define accepted programs, so PHP covariance, inheritance, and extension-specific constraints must stay explicit.

use crate::types::PhpType;

use super::super::Checker;

impl Checker {
    /// Flattens nested unions, removes duplicates and `PhpType::Mixed` (which absorbs all),
    /// and returns a single `PhpType` or a `PhpType::Union` with deduped members.
    pub(crate) fn normalize_union_type(&self, members: Vec<PhpType>) -> PhpType {
        let mut flat = Vec::new();
        for member in members {
            match member {
                PhpType::Union(inner) => flat.extend(inner),
                PhpType::Mixed => return PhpType::Mixed,
                other => flat.push(other),
            }
        }

        let mut deduped = Vec::new();
        for member in flat {
            if !deduped.iter().any(|existing| existing == &member) {
                deduped.push(member);
            }
        }

        if deduped.len() == 1 {
            deduped.pop().expect("union member exists")
        } else {
            PhpType::Union(deduped)
        }
    }

    /// Returns true if `expected` type can accept a value of `actual` type (i.e., the
    /// assignment `expected = actual` is valid). Checks identity, Mixed, unions, arrays,
    /// associative arrays, object class/interface compatibility, `iterable`, pointers,
    /// and resources.
    pub(crate) fn type_accepts(&self, expected: &PhpType, actual: &PhpType) -> bool {
        if expected == actual {
            return true;
        }

        match expected {
            PhpType::Mixed => true,
            PhpType::Union(members) => members
                .iter()
                .any(|member| self.type_accepts(member, actual)),
            PhpType::Array(expected_elem) => match actual {
                PhpType::Array(actual_elem) if matches!(actual_elem.as_ref(), PhpType::Never) => {
                    true
                }
                PhpType::Array(actual_elem) => {
                    self.type_accepts(expected_elem.as_ref(), actual_elem.as_ref())
                }
                PhpType::AssocArray { .. } => matches!(expected_elem.as_ref(), PhpType::Mixed),
                _ => false,
            },
            PhpType::AssocArray {
                key: expected_key,
                value: expected_value,
            } => match actual {
                PhpType::AssocArray {
                    key: actual_key,
                    value: actual_value,
                } => {
                    self.type_accepts(expected_key.as_ref(), actual_key.as_ref())
                        && self.type_accepts(expected_value.as_ref(), actual_value.as_ref())
                }
                PhpType::Array(actual_elem)
                    if matches!(expected_key.as_ref(), PhpType::Mixed)
                        && self.type_accepts(expected_value.as_ref(), actual_elem.as_ref()) =>
                {
                    true
                }
                _ => false,
            },
            PhpType::Object(expected_name) => match actual {
                PhpType::Object(actual_name) => {
                    expected_name == actual_name
                        || self.is_subclass_of(actual_name, expected_name)
                        || self.class_implements_interface(actual_name, expected_name)
                        || self.interface_extends_interface(actual_name, expected_name)
                }
                _ => false,
            },
            PhpType::Iterable => match actual {
                PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => true,
                PhpType::Object(actual_name) => self.object_type_implements_iterable(actual_name),
                _ => false,
            },
            PhpType::Pointer(_) => Self::pointer_types_compatible(expected, actual),
            PhpType::Resource(_) => PhpType::resource_types_compatible(expected, actual),
            _ => false,
        }
    }

    /// Returns true if `ty` is a `PhpType::Union` that contains `PhpType::Void`.
    pub(crate) fn union_contains_void(ty: &PhpType) -> bool {
        matches!(ty, PhpType::Union(members) if members.iter().any(|member| *member == PhpType::Void))
    }

    /// Removes `PhpType::Void` from a union type, re-normalizing the result.
    /// If all members are removed, returns `PhpType::Never`. If only one member remains,
    /// returns it directly without wrapping in a union.
    pub(crate) fn strip_void_from_union(&self, ty: &PhpType) -> PhpType {
        match ty {
            PhpType::Union(members) => {
                let filtered: Vec<PhpType> = members
                    .iter()
                    .filter(|member| **member != PhpType::Void)
                    .cloned()
                    .collect();
                self.normalize_union_type(filtered)
            }
            other => other.clone(),
        }
    }

    /// Returns true if `ty` is `Int`, `Bool`, `Void`, `Str`, or a union of only those types.
    /// These types support fast integer-dispatch in `Mixed` value handling.
    pub(crate) fn type_supports_mixed_int_dispatch(&self, ty: &PhpType) -> bool {
        let _ = self;
        match ty {
            PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Str => true,
            PhpType::Union(members) => members
                .iter()
                .all(|member| self.type_supports_mixed_int_dispatch(member)),
            _ => false,
        }
    }

    /// Returns true if `ty` is a union type where every member supports mixed-int dispatch.
    pub(crate) fn is_union_with_mixed_int_dispatch(&self, ty: &PhpType) -> bool {
        matches!(ty, PhpType::Union(_)) && self.type_supports_mixed_int_dispatch(ty)
    }

    /// Computes the merged type when assigning `new_ty` to a variable that already has
    /// `existing` type. Returns `Some(merged)` when types are compatible for compound assignment
    /// (e.g., `+=`), or `None` when the types cannot be merged (e.g., two incompatible scalars).
    pub(crate) fn merged_assignment_type(
        &self,
        existing: &PhpType,
        new_ty: &PhpType,
    ) -> Option<PhpType> {
        if self.type_accepts(existing, new_ty) {
            return Some(existing.clone());
        }
        if matches!(existing, PhpType::Union(_)) {
            return None;
        }
        if existing == new_ty {
            return Some(existing.clone());
        }
        if matches!(existing, PhpType::Array(inner) if matches!(inner.as_ref(), PhpType::Never))
            && matches!(new_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        {
            return Some(new_ty.clone());
        }
        if matches!(new_ty, PhpType::Array(inner) if matches!(inner.as_ref(), PhpType::Never))
            && matches!(existing, PhpType::Array(_) | PhpType::AssocArray { .. })
        {
            return Some(existing.clone());
        }
        if matches!(existing, PhpType::Mixed) || matches!(new_ty, PhpType::Mixed) {
            return Some(PhpType::Mixed);
        }
        if *new_ty == PhpType::Void {
            return Some(existing.clone());
        }
        if *existing == PhpType::Void {
            return Some(new_ty.clone());
        }
        if matches!(existing, PhpType::Int | PhpType::Bool | PhpType::Float)
            && matches!(new_ty, PhpType::Int | PhpType::Bool | PhpType::Float)
        {
            return Some(existing.clone());
        }
        if Self::pointer_types_compatible(existing, new_ty) {
            return Some(match (existing, new_ty) {
                (PhpType::Pointer(Some(left)), PhpType::Pointer(Some(right))) if left == right => {
                    PhpType::Pointer(Some(left.clone()))
                }
                (PhpType::Pointer(None), PhpType::Pointer(Some(tag)))
                | (PhpType::Pointer(Some(tag)), PhpType::Pointer(None)) => {
                    PhpType::Pointer(Some(tag.clone()))
                }
                _ => PhpType::Pointer(None),
            });
        }
        if PhpType::resource_types_compatible(existing, new_ty) {
            return Some(match (existing, new_ty) {
                (PhpType::Resource(Some(left)), PhpType::Resource(Some(right)))
                    if left == right =>
                {
                    PhpType::Resource(Some(left.clone()))
                }
                (PhpType::Resource(None), PhpType::Resource(Some(kind)))
                | (PhpType::Resource(Some(kind)), PhpType::Resource(None)) => {
                    PhpType::Resource(Some(kind.clone()))
                }
                _ => PhpType::Resource(None),
            });
        }
        None
    }

    /// Computes the merged array element type when writing a value of `new_ty` into an
    /// array that already has `existing` element type. Returns `Some(merged)` for compatible
    /// types (same, `Never`, `Mixed`, or compatible objects), or `None` otherwise.
    pub(crate) fn merge_array_element_type(
        &self,
        existing: &PhpType,
        new_ty: &PhpType,
    ) -> Option<PhpType> {
        if existing == new_ty {
            return Some(existing.clone());
        }
        if matches!(existing, PhpType::Never) {
            return Some(new_ty.clone());
        }
        if matches!(new_ty, PhpType::Never) {
            return Some(existing.clone());
        }
        if matches!(existing, PhpType::Mixed) || matches!(new_ty, PhpType::Mixed) {
            return Some(PhpType::Mixed);
        }

        match (existing, new_ty) {
            (PhpType::Object(left), PhpType::Object(right)) => self.common_object_type(left, right),
            _ => None,
        }
    }

    /// Returns true if `ty` is `PhpType::Array(Box::new(PhpType::Mixed))`, i.e., an
    /// untyped `array` hint without element type specialization.
    pub(crate) fn is_generic_array_hint(ty: &PhpType) -> bool {
        matches!(ty, PhpType::Array(inner) if matches!(inner.as_ref(), PhpType::Mixed))
    }

    /// If `declared_ty` is a generic array hint and `actual_ty` is a concrete array or
    /// assoc-array, returns `actual_ty` (specialization). Otherwise returns `declared_ty`.
    /// Used to sharpen untyped `array` parameters when the actual argument type is known.
    pub(crate) fn specialize_generic_array_hint(
        declared_ty: &PhpType,
        actual_ty: &PhpType,
    ) -> PhpType {
        if Self::is_generic_array_hint(declared_ty)
            && matches!(actual_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        {
            actual_ty.clone()
        } else {
            declared_ty.clone()
        }
    }
}
