//! Purpose:
//! Separates public Zend constant types from checked native numeric contracts.
//!
//! Called from:
//! - Constant registration, reference resolution and hosted schema materialization.
//!
//! Key details:
//! - Numeric widening uses the shared PhpType contract, including nominal union members.
//! - Native implementation types do not expand the admitted public metadata surface.

use crate::{parser::ast::TypeExpr, types::PhpType};

/// Resolves the supported constant type atoms through the shared PHP value contract.
pub(super) fn declared_type(ty: &TypeExpr) -> Result<PhpType, String> {
    Ok(match ty {
        TypeExpr::Int => PhpType::Int, TypeExpr::Float => PhpType::Float, TypeExpr::Bool => PhpType::Bool,
        TypeExpr::False => PhpType::False, TypeExpr::Str => PhpType::Str,
        TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("mixed") => PhpType::Mixed,
        TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("null") => PhpType::Void,
        TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("array") => PhpType::php_array(),
        TypeExpr::Nullable(inner) => PhpType::Union(vec![declared_type(inner)?, PhpType::Void]),
        TypeExpr::Union(members) => PhpType::Union(members.iter().map(declared_type).collect::<Result<Vec<_>, _>>()?),
        _ => return Err("unsupported PHP extension class constant type".into()),
    })
}

/// Shares native numeric binding independently of the Zend public metadata type subset.
pub(super) use crate::types::constant_binding::declared_type as coercion_type;
