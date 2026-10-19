//! Purpose:
//! Applies generic PHPDoc to properties and methods, including method-local templates.
//! Keeps class scope separate from templates bound by an individual method call.
//!
//! Called from:
//! - `crate::docblock::apply_to_stmt()` for class, interface, trait and enum members.
//!
//! Key details:
//! - Native method templates take precedence over PHPDoc.
//! - Ordinary member annotations only apply when they mention a class type parameter.
//! - A method template honours annotations naming its own or the class's type parameters.
//! - A bodiless (interface/abstract) method keeps its native signature: its PHPDoc types are
//!   claims about dynamic dispatch that php-src never enforces.
//! - Promoted properties retain the annotated constructor parameter's storage type.

use std::collections::HashMap;

use crate::parser::ast::{ClassMethod, ClassProperty, TypeExpr, TypeParam};

use super::DocBlock;

/// Applies class-scoped member annotations and adopts each method's own template declaration.
pub(super) fn apply(
    class_params: &[String],
    properties: &mut [ClassProperty],
    methods: &mut [ClassMethod],
    blocks: &HashMap<(u32, u32), DocBlock>,
) {
    for property in properties.iter_mut() {
        if let Some(annotated) = blocks
            .get(&(property.span.line, property.span.col))
            .and_then(|block| block.var_type.as_ref())
        {
            apply_type(&mut property.type_expr, annotated, class_params, &[]);
        }
    }
    let mut promoted = Vec::new();
    for method in methods.iter_mut() {
        let Some(block) = blocks.get(&(method.span.line, method.span.col)) else {
            continue;
        };
        if !method.type_params.is_empty() {
            continue;
        }
        // A method with no body (an interface or abstract method) is only ever reached through
        // dynamic dispatch, and an instantiated generic method never takes a vtable slot. Its
        // PHPDoc is therefore not adopted at all: the method keeps its native signature, so an
        // implementation that omits a type PHPDoc only claimed is still compatible, exactly as
        // php-src sees it.
        if !method.has_body {
            continue;
        }
        // The names an annotation must mention to be honoured: the class's parameters, plus the
        // method's own when it is a template. `@param string $key` beside a `@template` says
        // nothing generic, and adopting it would narrow a parameter the native signature (and
        // every interface the method implements) leaves untyped.
        let mut in_scope: Vec<String> = class_params.to_vec();
        let own: &[TypeParam] = if block.declares_generics() {
            method.type_params = block.type_params.clone();
            in_scope.extend(block.type_params.iter().map(|param| param.name.clone()));
            &block.type_params
        } else {
            &[]
        };
        let in_scope = in_scope.as_slice();
        let is_constructor = method.name.eq_ignore_ascii_case("__construct");
        for (name, declared, _, _) in method.params.iter_mut() {
            let Some(annotated) = block.params.get(name) else {
                continue;
            };
            if apply_type(declared, annotated, in_scope, own) && is_constructor {
                promoted.push((name.clone(), annotated.clone()));
            }
        }
        if let Some(annotated) = method
            .variadic
            .as_ref()
            .and_then(|name| block.params.get(name))
        {
            apply_type(&mut method.variadic_type, annotated, in_scope, own);
        }
        if let Some(annotated) = &block.return_type {
            apply_type(&mut method.return_type, annotated, in_scope, own);
        }
        // A template no position adopted (every annotation lost to a native hint) binds nothing
        // at a call and would only make the call unresolvable; php-src never saw it either.
        if !own.is_empty() {
            let names: Vec<String> = own.iter().map(|param| param.name.clone()).collect();
            let adopted = method
                .params
                .iter()
                .map(|(_, declared, _, _)| declared)
                .chain([&method.variadic_type, &method.return_type])
                .flatten()
                .any(|declared| declared.mentions_type_param(&names));
            if !adopted {
                method.type_params.clear();
            }
        }
    }
    // The parser creates a promoted parameter and its property separately. Both must agree.
    for property in properties.iter_mut().filter(|property| property.is_promoted) {
        if let Some((_, annotated)) = promoted.iter().find(|(name, _)| *name == property.name) {
            property.type_expr = Some(annotated.clone());
        }
    }
}

/// Applies an annotation only when it mentions a type parameter in scope (class or method) and
/// refines, rather than contradicts, the native hint.
///
/// PHPDoc may sharpen a hint php-src treats loosely (`array`, `iterable`, `callable`, `object`,
/// none at all, or the bare name of the class it parameterizes, or the bound of a method
/// template). A native `mixed` is kept. A native class or scalar hint is a contract php-src
/// enforces and every override is checked against, so it wins:
/// `getIterator(): Traversable` documented `@return ArrayIterator<TKey, TValue>` stays
/// `Traversable`.
fn apply_type(
    declared: &mut Option<TypeExpr>,
    annotated: &TypeExpr,
    in_scope: &[String],
    method_params: &[TypeParam],
) -> bool {
    if !annotated.mentions_type_param(in_scope)
        || !refines_native(declared.as_ref(), annotated, method_params)
    {
        return false;
    }
    *declared = Some(annotated.clone());
    true
}

/// Returns whether `annotated` may replace the native hint `native`.
///
/// `method_params` are the method's own PHPDoc templates: `@param T $m` with
/// `@template T of Model` refines a native `Model $m`, because every `T` the call can bind is a
/// `Model` (the bound is checked at the call), so the contract php-src enforces still holds.
fn refines_native(
    native: Option<&TypeExpr>,
    annotated: &TypeExpr,
    method_params: &[TypeParam],
) -> bool {
    let Some(native) = native else {
        return true;
    };
    let same_class = |a: &crate::names::Name, b: &crate::names::Name| {
        a.last_segment()
            .zip(b.last_segment())
            .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
    };
    match native {
        TypeExpr::Array(_) | TypeExpr::AssocArray { .. } | TypeExpr::Iterable => true,
        TypeExpr::Nullable(inner) => {
            let annotated = match annotated {
                TypeExpr::Nullable(inner) => inner.as_ref(),
                other => other,
            };
            refines_native(Some(inner), annotated, method_params)
        }
        TypeExpr::Named(name) => {
            let written = name.as_str().trim_start_matches('\\');
            // `mixed` is not weak: it is already the widest type, so the PHPDoc cannot sharpen it
            // without a later erasure (a dispatched method) having to guess whether to give the
            // `mixed` back or leave the position untyped, and only `mixed` keeps an
            // implementation of `JsonSerializable::jsonSerialize(): mixed` compatible.
            let weak = ["callable", "object", "closure", "array", "iterable"]
                .iter()
                .any(|weak| written.eq_ignore_ascii_case(weak));
            weak || match annotated {
                TypeExpr::GenericClass { name: generic, .. } => same_class(generic, name),
                // A method template bounded by the native class (`T of Model` on `Model`).
                TypeExpr::Named(param) => method_params.iter().any(|tp| {
                    tp.name == param.as_str()
                        && matches!(&tp.bound, Some(TypeExpr::Named(bound)) if same_class(bound, name))
                }),
                _ => false,
            }
        }
        _ => false,
    }
}
