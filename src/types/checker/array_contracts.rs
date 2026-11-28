//! Purpose:
//! Preserves source array key and element contracts before storage widening.
//!
//! Called from:
//! - Indexed/append writes and whole-local array replacements.
//!
//! Key details:
//! - Empty arrays select their contract at the first informative write.
//! - Explicit mixed/union elements stay open; scalar element kinds never coerce.

use super::Checker;
use crate::{errors::CompileError, source::SourceMode, span::Span, types::PhpType};
use elephc_semantics::StrictFeature;

impl Checker {
    /// Checks a normalized key and original element before changing a container's inferred type.
    pub(crate) fn require_array_write_contract(&self, receiver: &PhpType, key: &PhpType,
        value: &PhpType, span: Span) -> Result<(), CompileError> {
        if self.semantic_source_mode == SourceMode::Internal
            || !self.semantic_profile.contains(StrictFeature::ArrayTypes)
            || *value == PhpType::Never
            || self.array_write_contract_accepts(receiver, key, value) { return Ok(()); }
        Err(CompileError::new(span, &format!(
            "--strict-array-types: cannot store {key} key and {value} value in {receiver}; declare an explicit mixed or union contract")))
    }

    /// Reuses the ordinary PHP array-cast contract before checking splice replacement elements.
    pub(crate) fn require_array_splice_contract(&self, receiver: &PhpType,
        replacement: &PhpType, span: Span) -> Result<(), CompileError> {
        let cast = crate::types::array_cast_result_type(replacement);
        self.require_array_splice_elements(receiver, &cast, span)
    }

    /// Checks each possible replacement payload while treating empty replacements as no writes.
    fn require_array_splice_elements(&self, receiver: &PhpType, replacement: &PhpType,
        span: Span) -> Result<(), CompileError> {
        match replacement {
            PhpType::Array(element) | PhpType::AssocArray { value: element, .. } =>
                self.require_array_write_contract(receiver, &PhpType::Int, element, span),
            PhpType::Union(members) => {
                for member in members { self.require_array_splice_elements(receiver, member, span)?; }
                Ok(())
            }
            _ => self.require_array_write_contract(receiver, &PhpType::Int, &PhpType::Mixed, span),
        }
    }

    /// Checks whole-array replacement without imposing another feature's whole-binding rules.
    pub(crate) fn require_array_replacement_contract(&self, previous: &PhpType,
        incoming: &PhpType, span: Span) -> Result<(), CompileError> {
        if self.semantic_source_mode == SourceMode::Internal
            || !self.semantic_profile.contains(StrictFeature::ArrayTypes)
            || !matches!(previous, PhpType::Array(_) | PhpType::AssocArray { .. })
            || !matches!(incoming, PhpType::Array(_) | PhpType::AssocArray { .. })
            || self.array_component_accepts(previous, incoming) { return Ok(()); }
        Err(CompileError::new(span, &format!(
            "--strict-array-types: cannot replace {previous} with {incoming}; declare an explicit mixed or union contract")))
    }

    /// Keeps PHP's unrestricted array declaration separate from a concrete inferred layout.
    fn array_write_contract_accepts(&self, receiver: &PhpType, key: &PhpType, value: &PhpType) -> bool {
        if receiver.is_php_array() { return true; }
        match receiver {
            PhpType::Array(element) if **element == PhpType::Never => true,
            PhpType::Array(element) => self.array_component_accepts(&PhpType::Int, key)
                && self.array_component_accepts(element, value),
            PhpType::AssocArray { key: expected_key, value: expected_value } =>
                self.array_component_accepts(expected_key, key) && self.array_component_accepts(expected_value, value),
            PhpType::Union(members) => members.iter().all(|member| self.array_write_contract_accepts(member, key, value)),
            _ => true,
        }
    }

    /// Uses exact element kinds and existing nominal relationships without scalar weak binding.
    pub(super) fn array_component_accepts(&self, expected: &PhpType, actual: &PhpType) -> bool {
        if expected == actual || matches!(expected, PhpType::Mixed | PhpType::Never)
            || *actual == PhpType::Never { return true; }
        if let PhpType::Union(members) = actual {
            return members.iter().all(|member| self.array_component_accepts(expected, member));
        }
        if let PhpType::Union(members) = expected {
            return members.iter().any(|member| self.array_component_accepts(member, actual));
        }
        match (expected, actual) {
            (PhpType::Array(_) | PhpType::AssocArray { .. }, PhpType::Array(element))
                if **element == PhpType::Never => true,
            (PhpType::Array(element), PhpType::Array(_) | PhpType::AssocArray { .. })
                if **element == PhpType::Never => true,
            (PhpType::Array(element), PhpType::AssocArray { key, value }) =>
                self.array_component_accepts(&PhpType::Int, key) && self.array_component_accepts(element, value),
            (PhpType::AssocArray { key, value }, PhpType::Array(element)) =>
                self.array_component_accepts(key, &PhpType::Int) && self.array_component_accepts(value, element),
            (PhpType::Bool, PhpType::False) => true,
            (PhpType::Object(_), PhpType::Object(_)) | (PhpType::Resource(_), PhpType::Resource(_)) => self.type_accepts(expected, actual),
            (PhpType::Array(element), PhpType::Array(candidate)) => self.array_component_accepts(element, candidate),
            (PhpType::AssocArray { key, value }, PhpType::AssocArray { key: other_key, value: other_value }) =>
                self.array_component_accepts(key, other_key) && self.array_component_accepts(value, other_value),
            _ => false,
        }
    }
}
