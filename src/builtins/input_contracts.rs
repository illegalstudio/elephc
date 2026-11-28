//! Purpose:
//! Projects neutral builtin input declarations without erasing nullable alternatives.
//!
//! Called from:
//! - Source semantic validation before constant folding and typed builtin call adapters.
//!
//! Key details:
//! - Selected AOT metadata remains the only parameter and default authority.
//! - Output references and mixed input APIs retain their existing specialized contracts.

use super::{registry::BuiltinDef, spec::{DefaultSpec, TypeSpec}};
use crate::types::PhpType;

/// Returns an exact value-input contract while retaining explicit null default sentinels.
pub(crate) fn parameter_type(definition: &BuiltinDef, index: usize) -> Option<PhpType> {
    let parameter = definition.spec.params.get(index)?;
    if parameter.by_ref || parameter.ty == TypeSpec::Mixed { return None; }
    let mut ty = super::convert::type_spec_to_php_preserving_null(&parameter.ty);
    if parameter.default == Some(DefaultSpec::Null) && ty != PhpType::Void
        && !matches!(&ty, PhpType::Union(members) if members.contains(&PhpType::Void))
    {
        ty = PhpType::Union(vec![ty, PhpType::Void]);
    }
    Some(ty)
}
