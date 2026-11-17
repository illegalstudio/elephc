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
//! - An explicit method template applies its parameter and return annotations like a function.
//! - Promoted properties retain the annotated constructor parameter's storage type.

use std::collections::HashMap;

use crate::parser::ast::{ClassMethod, ClassProperty, TypeExpr, TypeParam};

use super::DocBlock;

/// Applies class-scoped member annotations and adopts each method's own template declaration.
pub(super) fn apply(
    class_params: &[String],
    class_own: &[TypeParam],
    properties: &mut [ClassProperty],
    methods: &mut [ClassMethod],
    blocks: &HashMap<(u32, u32), DocBlock>,
) {
    for property in properties.iter_mut() {
        if let Some(annotated) = blocks
            .get(&(property.span.line, property.span.col))
            .and_then(|block| block.var_type.as_ref())
        {
            apply_type(&mut property.type_expr, annotated, class_params, class_own, false);
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
        let is_template = block.declares_generics();
        let method_own = block.type_params.clone();
        // A method template that adopts nothing and that no written hint mentions is not a
        // template: php-src checks the written hints, and a template would ask the call for a
        // type argument PHP never requires.
        if is_template {
            let names: Vec<String> = method_own.iter().map(|param| param.name.clone()).collect();
            let adopts = method.params.iter().any(|(name, declared, _, _)| {
                block.params.get(name).is_some_and(|annotated| {
                    super::adopts_annotation(declared.as_ref(), annotated, &method_own)
                })
            }) || method
                .variadic
                .as_ref()
                .and_then(|name| block.params.get(name))
                .is_some_and(|annotated| {
                    super::adopts_annotation(method.variadic_type.as_ref(), annotated, &method_own)
                })
                || block.return_type.as_ref().is_some_and(|annotated| {
                    super::adopts_annotation(method.return_type.as_ref(), annotated, &method_own)
                });
            let written_mentions = method.params.iter().any(|(_, declared, _, _)| {
                declared.as_ref().is_some_and(|ty| ty.mentions_type_param(&names))
            }) || method
                .variadic_type
                .as_ref()
                .is_some_and(|ty| ty.mentions_type_param(&names))
                || method
                    .return_type
                    .as_ref()
                    .is_some_and(|ty| ty.mentions_type_param(&names));
            if !adopts && !written_mentions {
                continue;
            }
        }
        if is_template {
            method.type_params = method_own.clone();
        }
        // Ordinary member annotations may mention the class's own templates, so both sets take
        // part in the adoption check: `@param T $items` on a constructor documents `array $items`
        // through the class parameter `T`.
        let mut own = class_own.to_vec();
        own.extend(method_own.iter().cloned());
        let names: Vec<String> = own.iter().map(|param| param.name.clone()).collect();
        let is_constructor = method.name.eq_ignore_ascii_case("__construct");
        let mut parameter_binds = false;
        for (name, declared, _, _) in method.params.iter_mut() {
            let Some(annotated) = block.params.get(name) else {
                continue;
            };
            if declared.as_ref().is_some_and(|ty| ty.mentions_type_param(&names)) {
                parameter_binds = true;
            }
            if apply_type(declared, annotated, class_params, &own, is_template) {
                parameter_binds = true;
                if is_constructor {
                    promoted.push((name.clone(), declared.clone()));
                }
            }
        }
        if let Some(annotated) = method.variadic.as_ref().and_then(|name| block.params.get(name)) {
            if apply_type(&mut method.variadic_type, annotated, class_params, &own, is_template) {
                parameter_binds = true;
            }
        }
        if let Some(annotated) = &block.return_type {
            // A return annotation that mentions a METHOD template no parameter binds would ask
            // the call for a type argument inference cannot supply, so the written hint stays. A
            // CLASS template needs no parameter here: the construction binds it.
            let class_names: Vec<String> =
                class_own.iter().map(|param| param.name.clone()).collect();
            let class_bound = annotated.mentions_type_param(&class_names);
            if !annotated.mentions_type_param(&names) || parameter_binds || class_bound {
                apply_type(&mut method.return_type, annotated, class_params, &own, is_template);
            }
        }
    }
    // The parser creates a promoted parameter and its property separately. Both must agree.
    for property in properties.iter_mut().filter(|property| property.is_promoted) {
        if let Some((_, adopted)) = promoted.iter().find(|(name, _)| *name == property.name) {
            property.type_expr = adopted.clone();
        }
    }
}

/// Applies template annotations, retaining the class-parameter gate for ordinary members.
fn apply_type(
    declared: &mut Option<TypeExpr>,
    annotated: &TypeExpr,
    class_params: &[String],
    own: &[TypeParam],
    method_template: bool,
) -> bool {
    if !method_template && !annotated.mentions_type_param(class_params) {
        return false;
    }
    // A hint php-src enforces stays: `@param T $m` documenting `int $m` keeps `int`, and the
    // annotation only refines a loose hint such as `array`.
    if !super::adopts_annotation(declared.as_ref(), annotated, own) {
        return false;
    }
    let written = declared.take();
    *declared = Some(super::with_written_nullability(written.as_ref(), annotated.clone()));
    true
}
