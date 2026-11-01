//! Purpose:
//! Shared rules for deciding which PHPDoc templates and annotations a function adopts.
//!
//! Called from:
//! - `crate::docblock::doc_twin()` for free functions.
//!
//! Key details:
//! - Method, class and constructor templates are NOT adopted (see the module doc of
//!   `crate::docblock`): a PHPDoc template must never remove or rename a php symbol.
//! - A function annotation only applies when it mentions one of the function's own templates.
//! - A `@template` no read annotation mentions is not adopted.

use crate::parser::ast::TypeExpr;

use super::DocBlock;

/// The function's `@template` parameters that one of its read annotations actually mentions.
///
/// A template only PHPStan or Psalm vocabulary names (Symfony's `setParent()` declares
/// `@template TNewParent` for a `@psalm-this-out static<TNewParent>` this reader does not read)
/// binds nothing at a call, and adopting it would still turn the method into a template whose
/// instantiation copies `static` out of its class. A parameter a kept one's bound or default
/// names is kept with it.
pub(super) fn mentioned_templates(
    block: &DocBlock,
    params: &[(String, Option<TypeExpr>, Option<crate::parser::ast::Expr>, bool)],
    variadic: Option<&String>,
    variadic_type: Option<&TypeExpr>,
    return_type: Option<&TypeExpr>,
) -> Vec<crate::parser::ast::TypeParam> {
    let all: Vec<String> = block.type_params.iter().map(|param| param.name.clone()).collect();
    let annotations = params
        .iter()
        .map(|(name, ..)| name)
        .chain(variadic)
        .filter_map(|name| block.params.get(name))
        // A return annotation is read only where it is adopted (see `apply_stored_type`).
        .chain(block.return_type.as_ref().filter(|ty| mentions_directly(ty, &all)))
        // Written types count too: `/** @template T */ function id(T $v): T` names it natively.
        .chain(params.iter().filter_map(|(_, ty, _, _)| ty.as_ref()))
        .chain(variadic_type)
        .chain(return_type);
    let mut kept: Vec<String> = Vec::new();
    for annotated in annotations {
        for name in &all {
            if !kept.contains(name) && annotated.mentions_type_param(std::slice::from_ref(name)) {
                kept.push(name.clone());
            }
        }
    }
    loop {
        let before = kept.len();
        for param in &block.type_params {
            if !kept.contains(&param.name) {
                continue;
            }
            for name in &all {
                let named = param.bound.iter().chain(param.default.iter())
                    .any(|ty| ty.mentions_type_param(std::slice::from_ref(name)));
                if named && !kept.contains(name) {
                    kept.push(name.clone());
                }
            }
        }
        if kept.len() == before {
            break;
        }
    }
    block
        .type_params
        .iter()
        .filter(|param| kept.contains(&param.name))
        .cloned()
        .collect()
}

/// Adopts an annotation on a position the BODY fills (a return, a property) only where it
/// names a type parameter directly: `T`, `?T`, `T|null`.
///
/// Inside a container (`@return array<T>`) the annotation would fix the element storage of
/// values the body builds from untyped data — Symfony's `getAttributesOfType()` collects
/// `$this->attributes` into an `array` declared `@return array<T>`, refused as "the element
/// storage differs". php reads the written type there, so that is what is kept. A parameter is
/// different: its type is bound FROM the argument, so `@param array<T> $items` stays adopted.
pub(super) fn apply_stored_type(
    declared: &mut Option<TypeExpr>,
    annotated: &TypeExpr,
    in_scope: &[String],
) -> bool {
    if !mentions_directly(annotated, in_scope) {
        return false;
    }
    apply_type(declared, annotated, in_scope)
}

/// Whether `ty` names one of `params` outside every container.
pub(super) fn mentions_directly(ty: &TypeExpr, params: &[String]) -> bool {
    match ty {
        TypeExpr::Named(_) => ty.mentions_type_param(params),
        TypeExpr::Nullable(inner) => mentions_directly(inner, params),
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            members.iter().any(|member| mentions_directly(member, params))
        }
        _ => false,
    }
}

/// Adopts an annotation only where it mentions a type parameter in scope.
pub(super) fn apply_type(declared: &mut Option<TypeExpr>, annotated: &TypeExpr, in_scope: &[String]) -> bool {
    if !annotated.mentions_type_param(in_scope) {
        return false;
    }
    *declared = Some(annotated.clone());
    true
}
