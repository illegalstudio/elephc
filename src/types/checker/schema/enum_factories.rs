//! Purpose:
//! Exposes the PHP-visible contract of a synthesized backed-enum factory for validation.
//!
//! Called from:
//! - Enum interface and imported abstract-trait requirement validation.
//!
//! Key details:
//! - The public input is int|string; concrete backing storage remains the executable ABI.
//! - A concrete source method is never mistaken for the synthesized implementation.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::ClassMethod;
use crate::types::{FunctionSig, PhpType};
use super::super::Checker;
use super::validation::{build_method_sig, late_static_return_compatible, validate_abstract_trait_signature};

/// Clones the public factory contract without changing its concrete backing-type ABI.
pub(super) fn public_signature(checker: &Checker, owner: &str, method: &str) -> Option<FunctionSig> {
    let method = php_symbol_key(method);
    if !matches!(method.as_str(), "from" | "tryfrom")
        || checker.enums.get(owner)?.backing_type.is_none()
    { return None; }
    let info = checker.classes.get(owner)?;
    if info.method_decls.iter().any(|decl| decl.is_static && php_symbol_key(&decl.name) == method) {
        return None;
    }
    let mut signature = info.static_methods.get(&method)?.clone();
    signature.params.first_mut()?.1 = PhpType::Union(vec![PhpType::Int, PhpType::Str]);
    Some(signature)
}

/// Checks an imported abstract requirement against the actual builtin factory's public surface.
pub(super) fn validate_trait_requirement(
    checker: &Checker, owner: &str, required: &ClassMethod, actual: &FunctionSig,
) -> Result<(), CompileError> {
    let mut required = required.clone();
    required.substitute_relative_class_types(owner, None);
    let required_sig = build_method_sig(checker, &required, owner)?;
    validate_abstract_trait_signature(checker, required.span, owner, &required.name, actual, &required_sig)?;
    if required_sig.by_ref_return && !actual.by_ref_return {
        return Err(CompileError::new(required.span, &format!(
            "Cannot remove by-reference return when implementing trait method: {owner}::{}", required.name,
        )));
    }
    let late_static = late_static_return_compatible(
        checker, required.return_type.as_ref().filter(|hint| hint.contains_late_static()),
        None, &actual.return_type, owner, required.span,
    )?;
    if required_sig.declared_return && !late_static.unwrap_or_else(|| {
        super::class_constants::strict_type_accepts(checker, &required_sig.return_type, &actual.return_type, false)
    }) {
        return Err(CompileError::new(required.span, &format!(
            "Enum method {owner}::{} has incompatible return type for abstract trait method", required.name,
        )));
    }
    Ok(())
}
