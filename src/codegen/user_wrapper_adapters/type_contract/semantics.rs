//! Purpose:
//! Resolves declared callback type expressions against static types and normalized Mixed tags.
//! Keeps PHP scalar preference and class/interface metadata lookup separate from assembly emission.
//!
//! Called from:
//! - `crate::codegen::user_wrapper_adapters::type_contract`.
//!
//! Key details:
//! - Exact union members are tested before weak scalar fallbacks.
//! - Named class/interface matching is case-insensitive and follows compiled inheritance metadata.

use crate::ir::Module;
use crate::names::php_symbol_key;
use crate::parser::ast::TypeExpr;
use crate::types::PhpType;

/// Returns the weak scalar fallback selected after exact union matches have failed.
pub(super) fn scalar_fallback(type_expr: &TypeExpr, source_tag: u8) -> Option<PhpType> {
    let preference = match source_tag {
        0 => [ScalarAtom::Float, ScalarAtom::String, ScalarAtom::Bool],
        1 => [ScalarAtom::Int, ScalarAtom::Float, ScalarAtom::Bool],
        2 => [ScalarAtom::Int, ScalarAtom::String, ScalarAtom::Bool],
        3 => [ScalarAtom::Int, ScalarAtom::Float, ScalarAtom::String],
        _ => return None,
    };
    preference
        .into_iter()
        .find(|atom| type_expr_has_atom(type_expr, *atom))
        .map(ScalarAtom::php_type)
}

/// Returns whether a runtime tag is accepted without inspecting its payload or object metadata.
pub(super) fn type_expr_accepts_tag_without_value(type_expr: &TypeExpr, tag: u8) -> bool {
    match type_expr {
        TypeExpr::Int => tag == 0,
        TypeExpr::Float => tag == 2,
        TypeExpr::Bool => tag == 3,
        TypeExpr::False => false,
        TypeExpr::Str => tag == 1,
        TypeExpr::Void => tag == 8,
        TypeExpr::Iterable => matches!(tag, 4 | 5),
        TypeExpr::Array(_) => matches!(tag, 4 | 5),
        TypeExpr::Nullable(inner) => {
            tag == 8 || type_expr_accepts_tag_without_value(inner, tag)
        }
        TypeExpr::Union(members) => members
            .iter()
            .any(|member| type_expr_accepts_tag_without_value(member, tag)),
        TypeExpr::Intersection(_) => false,
        TypeExpr::Named(name) => match name.as_str().to_ascii_lowercase().as_str() {
            "mixed" => true,
            "array" => matches!(tag, 4 | 5),
            "object" => tag == 6,
            "callable" | "closure" => tag == 10,
            "string" => tag == 1,
            "void" | "null" => tag == 8,
            _ => false,
        },
        TypeExpr::Never | TypeExpr::Ptr(_) | TypeExpr::Buffer(_) => false,
    }
}

/// Returns whether a runtime tag may satisfy a value-sensitive declared atom.
pub(super) fn type_expr_has_value_checked_candidate(type_expr: &TypeExpr, tag: u8) -> bool {
    match type_expr {
        TypeExpr::False => tag == 3,
        TypeExpr::Iterable => tag == 6,
        TypeExpr::Nullable(inner) => type_expr_has_value_checked_candidate(inner, tag),
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => members
            .iter()
            .any(|member| type_expr_has_value_checked_candidate(member, tag)),
        TypeExpr::Named(name) => {
            let raw = name.as_str();
            (raw.eq_ignore_ascii_case("callable") && matches!(tag, 1 | 4 | 5 | 6))
                || (tag == 6 && !is_builtin_named_type(raw))
        }
        _ => false,
    }
}

/// Returns whether a statically known source exactly satisfies the declaration.
pub(super) fn type_expr_accepts_static_exact(
    module: &Module,
    type_expr: &TypeExpr,
    source_ty: &PhpType,
) -> bool {
    match type_expr {
        TypeExpr::Int => source_ty.codegen_repr() == PhpType::Int,
        TypeExpr::Float => source_ty.codegen_repr() == PhpType::Float,
        TypeExpr::Bool => matches!(source_ty.codegen_repr(), PhpType::Bool),
        TypeExpr::False => matches!(source_ty, PhpType::False),
        TypeExpr::Str => source_ty.codegen_repr() == PhpType::Str,
        TypeExpr::Void => source_ty.codegen_repr() == PhpType::Void,
        TypeExpr::Iterable => matches!(
            source_ty,
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable
        ),
        TypeExpr::Array(_) => {
            matches!(source_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        }
        TypeExpr::Nullable(inner) => {
            source_ty.codegen_repr() == PhpType::Void
                || type_expr_accepts_static_exact(module, inner, source_ty)
        }
        TypeExpr::Union(members) => members
            .iter()
            .any(|member| type_expr_accepts_static_exact(module, member, source_ty)),
        TypeExpr::Intersection(members) => members
            .iter()
            .all(|member| type_expr_accepts_static_exact(module, member, source_ty)),
        TypeExpr::Named(name) => {
            let raw = name.as_str();
            match raw.to_ascii_lowercase().as_str() {
                "mixed" => true,
                "array" => {
                    matches!(source_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
                }
                "object" => matches!(source_ty, PhpType::Object(_)),
                "callable" | "closure" => source_ty.codegen_repr() == PhpType::Callable,
                "string" => source_ty.codegen_repr() == PhpType::Str,
                "void" | "null" => source_ty.codegen_repr() == PhpType::Void,
                _ => static_object_matches(module, source_ty, raw),
            }
        }
        TypeExpr::Never | TypeExpr::Ptr(_) | TypeExpr::Buffer(_) => false,
    }
}

/// Resolves a named class/interface to the runtime matcher metadata.
pub(super) fn classify_named_target(module: &Module, name: &str) -> Option<(u64, i64)> {
    let key = php_symbol_key(name.trim_start_matches('\\'));
    if let Some((_, info)) = module
        .class_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == key)
    {
        return Some((info.class_id, 0));
    }
    module
        .interface_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == key)
        .map(|(_, info)| (info.interface_id, 1))
}

/// Returns whether the declaration is PHP's unconstrained mixed atom.
pub(super) fn type_expr_is_mixed(type_expr: &TypeExpr) -> bool {
    matches!(
        type_expr,
        TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("mixed")
    )
}

/// Returns whether a declaration contains one scalar atom.
pub(super) fn type_expr_has_atom(type_expr: &TypeExpr, atom: ScalarAtom) -> bool {
    match type_expr {
        TypeExpr::Nullable(inner) => type_expr_has_atom(inner, atom),
        TypeExpr::Union(members) => members
            .iter()
            .any(|member| type_expr_has_atom(member, atom)),
        TypeExpr::Int => atom == ScalarAtom::Int,
        TypeExpr::Float => atom == ScalarAtom::Float,
        TypeExpr::Bool => atom == ScalarAtom::Bool,
        TypeExpr::False => atom == ScalarAtom::False,
        TypeExpr::Str => atom == ScalarAtom::String,
        TypeExpr::Named(name) => match name.as_str().to_ascii_lowercase().as_str() {
            "string" => atom == ScalarAtom::String,
            _ => false,
        },
        _ => false,
    }
}

/// Returns whether a named type is a builtin rather than class/interface metadata.
pub(super) fn is_builtin_named_type(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "mixed" | "array" | "object" | "callable" | "closure" | "string" | "void" | "null"
    )
}

/// Maps one normalized Mixed tag to its diagnostic PHP type.
pub(super) fn php_type_for_runtime_tag(tag: u8) -> PhpType {
    match tag {
        0 => PhpType::Int,
        1 => PhpType::Str,
        2 => PhpType::Float,
        3 => PhpType::Bool,
        4 => PhpType::Array(Box::new(PhpType::Mixed)),
        5 => PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
        6 => PhpType::Object("object".to_string()),
        8 => PhpType::Void,
        9 => PhpType::Resource(None),
        10 => PhpType::Callable,
        _ => PhpType::Mixed,
    }
}

/// Returns whether one known object type satisfies a named class/interface declaration.
fn static_object_matches(module: &Module, source_ty: &PhpType, target: &str) -> bool {
    let PhpType::Object(source) = source_ty else {
        return false;
    };
    let target_key = php_symbol_key(target.trim_start_matches('\\'));
    let mut current = Some(source.as_str());
    while let Some(class_name) = current {
        if php_symbol_key(class_name.trim_start_matches('\\')) == target_key {
            return true;
        }
        let Some((_, info)) = module.class_infos.iter().find(|(candidate, _)| {
            php_symbol_key(candidate.trim_start_matches('\\'))
                == php_symbol_key(class_name.trim_start_matches('\\'))
        }) else {
            break;
        };
        if info
            .interfaces
            .iter()
            .any(|interface| php_symbol_key(interface.trim_start_matches('\\')) == target_key)
        {
            return true;
        }
        current = info.parent.as_deref();
    }
    false
}

/// Scalar atoms participating in PHP's weak union-selection preference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ScalarAtom {
    Int,
    Float,
    String,
    Bool,
    False,
}

impl ScalarAtom {
    /// Returns the semantic PHP type represented by one scalar atom.
    fn php_type(self) -> PhpType {
        match self {
            Self::Int => PhpType::Int,
            Self::Float => PhpType::Float,
            Self::String => PhpType::Str,
            Self::Bool => PhpType::Bool,
            Self::False => PhpType::False,
        }
    }
}
