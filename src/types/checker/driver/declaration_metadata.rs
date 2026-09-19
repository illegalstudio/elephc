//! Purpose:
//! Collects declaration metadata and resolves class-relative types before checker schema construction.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()`.
//!
//! Key details:
//! - Trait and enum metadata must reflect flattened declarations without losing source signatures.

use std::collections::{HashMap, HashSet};

use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Program, Stmt, StmtKind};
use crate::types::{
    callable_wrapper_sig,
    traits::FlattenedClass,
    FunctionSig, PhpType,
};

/// Collects source-declared trait names recursively, including namespace blocks.
pub(super) fn collect_declared_trait_names(program: &Program) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_declared_trait_names_into(program, &mut names);
    names
}

/// Pushes recursive source-declared trait names into `names`.
fn collect_declared_trait_names_into(program: &Program, names: &mut HashSet<String>) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl { name, .. } => {
                names.insert(name.clone());
            }
            StmtKind::NamespaceBlock { body, .. } => {
                collect_declared_trait_names_into(body, names);
            }
            _ => {}
        }
    }
}

/// Collects source-declared trait method signatures recursively, including namespace blocks.
pub(super) fn collect_declared_trait_methods(
    program: &Program,
) -> HashMap<String, HashMap<String, FunctionSig>> {
    let mut methods = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                methods: trait_methods,
                ..
            } => {
                methods.insert(
                    name.clone(),
                    trait_methods
                        .iter()
                        .map(|method| {
                            (
                                php_symbol_key(&method.name),
                                trait_method_reflection_sig(method),
                            )
                        })
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                methods.extend(collect_declared_trait_methods(body));
            }
            _ => {}
        }
    }
    methods
}

/// Collects source-declared trait constant names recursively, including namespace blocks.
pub(super) fn collect_declared_trait_constants(program: &Program) -> HashMap<String, HashSet<String>> {
    let mut constants = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                constants: trait_constants,
                ..
            } => {
                constants.insert(
                    name.clone(),
                    trait_constants
                        .iter()
                        .map(|constant| constant.name.clone())
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                constants.extend(collect_declared_trait_constants(body));
            }
            _ => {}
        }
    }
    constants
}

/// Builds the reflection-visible signature for a direct trait method.
///
/// Trait direct reflection only needs parameter names, defaults, by-reference
/// flags, variadic shape, and declared-type presence; class-relative type names
/// are resolved when the trait is flattened into a concrete class.
fn trait_method_reflection_sig(method: &ClassMethod) -> FunctionSig {
    let params = method
        .params
        .iter()
        .map(|(name, type_ann, _, _)| {
            (
                name.clone(),
                if type_ann.is_some() {
                    PhpType::Mixed
                } else {
                    PhpType::Int
                },
            )
        })
        .collect();
    let defaults = method
        .params
        .iter()
        .map(|(_, _, default, _)| default.clone())
        .collect();
    let mut ref_params: Vec<bool> = method
        .params
        .iter()
        .map(|(_, _, _, by_ref)| *by_ref)
        .collect();
    if method.variadic.is_some() {
        ref_params.push(method.variadic_by_ref);
    }
    callable_wrapper_sig(&FunctionSig {
        params,
        param_type_exprs: method
            .params
            .iter()
            .map(|(_, type_ann, _, _)| type_ann.clone())
            .chain(method.variadic.iter().map(|_| method.variadic_type.clone()))
            .collect(),
        param_attributes: method.param_attributes.clone(),
        defaults,
        return_type: PhpType::Mixed,
        declared_return: method.return_type.is_some(),
        by_ref_return: method.by_ref_return,
        ref_params,
        declared_params: method
            .params
            .iter()
            .map(|(_, type_ann, _, _)| type_ann.is_some())
            .chain(
                method
                    .variadic
                    .iter()
                    .map(|_| method.variadic_type.is_some()),
            )
            .collect(),
        variadic: method.variadic.clone(),
        deprecation: None,
        // Taken from the SOURCE body, as for free functions and closures.
        is_generator: crate::types::checker::yield_validation::body_contains_yield(&method.body),
    })
}

/// Builds method-checkable `FlattenedClass` units for every `enum` in the program so their method
/// bodies go through the same validation as class methods. Enum signatures are already registered
/// in `checker.classes` by the enum schema pass; these units only carry the names and method
/// bodies the method-check pass needs. The relative types `self`/`static` resolve to the enum
/// itself (enums have no parent).
pub(super) fn flatten_enum_methods(
    program: &[Stmt],
    flattened_enums: &HashMap<String, FlattenedClass>,
) -> Vec<FlattenedClass> {
    let mut units = Vec::new();
    for stmt in program {
        if let StmtKind::EnumDecl {
            name,
            implements,
            methods,
            constants,
            ..
        } = &stmt.kind
        {
            if let Some(flattened) = flattened_enums.get(name) {
                units.push(flattened.clone());
                continue;
            }
            let mut flattened = FlattenedClass {
                name: name.clone(),
                span: stmt.span,
                extends: None,
                implements: implements
                    .iter()
                    .map(|name| name.as_str().to_string())
                    .collect(),
                is_abstract: false,
                is_final: true,
                is_readonly_class: false,
                properties: Vec::new(),
                methods: methods.clone(),
                attributes: stmt.attributes.clone(),
                constants: constants.clone(),
                used_traits: Vec::new(),
                trait_aliases: Vec::new(),
            };
            substitute_relative_class_types_in_methods(&mut flattened.methods, name, None);
            units.push(flattened);
        }
    }
    units
}

/// Resolves the relative class types `self`/`static`/`parent` to concrete class names across
/// every flattened class's method parameter, method return, and property type annotations.
///
/// `self`/`static` resolve to the flattened class itself and `parent` to its `extends` target.
/// Because trait methods are already merged into the using class at this point, a trait method's
/// `self` correctly resolves to the using class rather than the trait. Annotations with no
/// relative type are left untouched.
pub(super) fn substitute_relative_class_types_in_flattened(classes: &mut [FlattenedClass]) {
    for class in classes.iter_mut() {
        let self_class = class.name.clone();
        let parent = class.extends.clone();
        let parent_ref = parent.as_deref();
        substitute_relative_class_types_in_methods(&mut class.methods, &self_class, parent_ref);
        for property in class.properties.iter_mut() {
            if let Some(ty) = property.type_expr.as_mut() {
                *ty = ty.substitute_relative_class_types(&self_class, parent_ref);
            }
        }
        substitute_relative_class_types_in_constants(
            &mut class.constants,
            &self_class,
            parent_ref,
        );
    }
}

/// Resolves relative class types inside flattened enum methods.
pub(super) fn substitute_relative_class_types_in_flattened_enums(enums: &mut HashMap<String, FlattenedClass>) {
    for enum_unit in enums.values_mut() {
        let self_class = enum_unit.name.clone();
        substitute_relative_class_types_in_methods(&mut enum_unit.methods, &self_class, None);
        substitute_relative_class_types_in_constants(&mut enum_unit.constants, &self_class, None);
    }
}

/// Rewrites `self`/`static`/`parent` type annotations on class constants after
/// composition and inheritance have established the concrete owner.
pub(super) fn substitute_relative_class_types_in_constants(
    constants: &mut [crate::parser::ast::ClassConst],
    self_class: &str,
    parent: Option<&str>,
) {
    for constant in constants {
        if let Some(type_expr) = constant.type_expr.as_mut() {
            *type_expr = type_expr.substitute_relative_class_types(self_class, parent);
        }
    }
}

/// Rewrites `self`/`static`/`parent` type annotations on a slice of methods by delegating to
/// `ClassMethod::substitute_relative_class_types`.
///
/// Used for user classes after trait/inheritance flattening, interfaces, and enums.
pub(super) fn substitute_relative_class_types_in_methods(
    methods: &mut [ClassMethod],
    self_class: &str,
    parent: Option<&str>,
) {
    for method in methods.iter_mut() {
        method.substitute_relative_class_types(self_class, parent);
    }
}
