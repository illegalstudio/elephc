//! Purpose:
//! Defines flattened trait and class member models used by type checking.
//! Coordinates trait expansion, merge rules, and validation before class schemas become final.
//!
//! Called from:
//! - `crate::types::checker::schema::classes`
//!
//! Key details:
//! - Trait composition must preserve PHP conflict, aliasing, visibility, and abstract-method requirements.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{
    ClassConst, ClassMethod, ClassProperty, Program, StmtKind, TraitUse,
};
use crate::span::Span;

mod expand;
mod merge;
mod validation;

#[derive(Debug, Clone)]
/// A class with all traits fully expanded, property/method conflicts resolved,
/// and direct members merged. Produced by `flatten_classes` and consumed by
/// schema building, type checking, and codegen.
pub struct FlattenedClass {
    pub name: String,
    pub extends: Option<String>,
    pub implements: Vec<String>,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_readonly_class: bool,
    pub properties: Vec<ClassProperty>,
    pub methods: Vec<ClassMethod>,
    pub attributes: Vec<crate::parser::ast::AttributeGroup>,
    pub constants: Vec<ClassConst>,
    pub used_traits: Vec<String>,
}

#[derive(Clone)]
/// Raw declaration data for a trait encountered during program traversal.
/// Stored in `trait_map` until `expand_trait` resolves its trait_uses and merges members.
struct TraitDeclInfo {
    trait_uses: Vec<TraitUse>,
    properties: Vec<ClassProperty>,
    methods: Vec<ClassMethod>,
    span: Span,
}

#[derive(Clone)]
/// Cached result of fully expanding a trait: all properties and methods
/// after applying trait_uses, conflict resolution, and adaptations.
/// Stored in the expansion cache to avoid repeated work.
struct ExpandedTrait {
    properties: Vec<ClassProperty>,
    methods: Vec<ClassMethod>,
}

#[derive(Clone)]
/// A trait method imported during trait composition, with its source trait
/// tracked for insteadof conflict resolution and visibility override resolution.
struct ImportedMethod {
    source_trait: String,
    decl: ClassMethod,
}

/// Scans `program` for all traits and classes, validates direct member uniqueness,
/// expands trait uses for each class, merges imported vs. local members, and returns
/// a vector of `FlattenedClass` with any composition errors collected.
///
/// Trait declarations are stored in `trait_map` for later expansion.
/// Classes with traits are processed in program order; each class's trait uses are
/// resolved recursively, then merged with the class's own members.
/// Circular trait composition and duplicate declarations are reported as errors.
/// Returns `([FlattenedClass], Vec<CompileError>)`.
pub fn flatten_classes(program: &Program) -> (Vec<FlattenedClass>, Vec<CompileError>) {
    let mut trait_map = HashMap::new();
    let mut trait_keys = HashSet::new();
    let mut class_like_keys = HashSet::new();
    let mut errors = Vec::new();

    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                trait_uses,
                properties,
                methods,
                constants: _,
            } => {
                let trait_key = php_symbol_key(name);
                if class_like_keys.contains(&trait_key) || !trait_keys.insert(trait_key) {
                    errors.push(CompileError::new(
                        stmt.span,
                        &format!("Duplicate trait declaration: {}", name),
                    ));
                    continue;
                }
                trait_map.insert(
                    name.clone(),
                    TraitDeclInfo {
                        trait_uses: trait_uses.clone(),
                        properties: properties.clone(),
                        methods: methods.clone(),
                        span: stmt.span,
                    },
                );
            }
            StmtKind::ClassDecl { name, .. }
            | StmtKind::EnumDecl { name, .. }
            | StmtKind::InterfaceDecl { name, .. } => {
                let class_like_key = php_symbol_key(name);
                if trait_keys.contains(&class_like_key) {
                    errors.push(CompileError::new(
                        stmt.span,
                        &format!("Duplicate class or interface declaration: {}", name),
                    ));
                    continue;
                }
                class_like_keys.insert(class_like_key);
            }
            _ => {}
        }
    }

    let mut cache = HashMap::new();
    let mut stack = Vec::new();
    let mut flattened = Vec::new();
    for stmt in program {
        if let StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_final,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
            constants,
        } = &stmt.kind
        {
            if let Err(error) = validation::validate_direct_members(properties, methods, stmt.span, name) {
                errors.extend(error.flatten());
                continue;
            }
            let (imported_props, imported_methods) = match expand::resolve_trait_uses(
                trait_uses,
                &trait_map,
                &mut cache,
                &mut stack,
                &format!("class {}", name),
                stmt.span,
            ) {
                Ok(result) => result,
                Err(error) => {
                    errors.extend(error.flatten());
                    continue;
                }
            };
            let merged_props = match merge::merge_properties(
                &imported_props,
                properties,
                stmt.span,
                &format!("class {}", name),
                true,
            ) {
                Ok(props) => props,
                Err(error) => {
                    errors.extend(error.flatten());
                    continue;
                }
            };
            let merged_methods = match merge::merge_methods(
                imported_methods,
                methods,
                stmt.span,
                &format!("class {}", name),
            ) {
                Ok(methods) => methods,
                Err(error) => {
                    errors.extend(error.flatten());
                    continue;
                }
            };
            let (merged_props, merged_methods) =
                crate::magic_constants::bind_trait_class_constants(
                    merged_props,
                    merged_methods,
                    name,
                );
            flattened.push(FlattenedClass {
                name: name.clone(),
                extends: extends.as_ref().map(|name| name.as_str().to_string()),
                implements: implements.iter().map(|name| name.as_str().to_string()).collect(),
                is_abstract: *is_abstract,
                is_final: *is_final,
                is_readonly_class: *is_readonly_class,
                properties: merged_props,
                methods: merged_methods,
                attributes: stmt.attributes.clone(),
                constants: constants.clone(),
                used_traits: trait_uses
                    .iter()
                    .flat_map(|use_decl| {
                        use_decl
                            .trait_names
                            .iter()
                            .map(|name| name.as_str().to_string())
                    })
                    .collect(),
            });
        }
    }

    (flattened, errors)
}
