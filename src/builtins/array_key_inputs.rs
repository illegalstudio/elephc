//! Purpose:
//! Declares original-key inputs shared by checking, source proof and builtin lowering.
//!
//! Called from:
//! - Builtin home descriptors and common semantic consumers.
//!
//! Key details:
//! - Indexes refer to normalized caller-visible parameters.
//! - Null column selection is distinct from using null as a PHP array key.

use crate::{errors::CompileError, span::Span, types::PhpType};
use elephc_semantics::{SemanticProfile, StrictFeature, conversions::ConversionPlan};

/// Original key input selected by a builtin's home descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrayKeyInput {
    /// One key applied to an array argument; null may select a whole column instead.
    Value { argument: usize, receiver: usize, nullable: bool },
    /// Values of an input array become keys in a newly constructed array.
    Values { argument: usize },
}

impl ArrayKeyInput {
    /// Returns the parameter containing original keys before any builtin conversion.
    pub fn argument(self) -> usize {
        match self { Self::Value { argument, .. } | Self::Values { argument } => argument }
    }

    /// Checks proven source kinds without guessing a boxed value or changing argument order.
    pub(crate) fn validate_type(self, profile: SemanticProfile, ty: &PhpType, span: Span) -> Result<(), CompileError> {
        if !profile.contains(StrictFeature::ArrayKeys) { return Ok(()); }
        let (actual, nullable) = match self {
            Self::Value { nullable, .. } => (ty, nullable),
            Self::Values { .. } => match ty {
                PhpType::Array(element) | PhpType::AssocArray { value: element, .. } => (element.as_ref(), false),
                _ => return Ok(()),
            },
        };
        let mut allowed = vec![PhpType::Int, PhpType::Str];
        if nullable { allowed.push(PhpType::Void); }
        if matches!(crate::semantics::conversions::membership(profile, crate::source::SourceMode::Php,
            StrictFeature::ArrayKeys, &PhpType::Union(allowed), actual), ConversionPlan::Reject(_))
        { return Err(CompileError::new(span, crate::semantics::array_keys::EXACT_KEY_MESSAGE)); }
        Ok(())
    }
}
