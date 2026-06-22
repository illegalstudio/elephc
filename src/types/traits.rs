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
    ClassConst, ClassMethod, ClassProperty, Program, StmtKind, TraitAdaptation, TraitUse,
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
    pub trait_aliases: Vec<(String, String)>,
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

/// Scans `program` for all traits, classes, and enums, validates direct member uniqueness,
/// expands trait uses for each class-like declaration, and returns flattened metadata with
/// any composition errors collected.
///
/// Trait declarations are stored in `trait_map` for later expansion.
/// Class-like declarations with traits are processed in program order; each declaration's trait
/// uses are resolved recursively, then merged with the declaration's own members.
/// Circular trait composition and duplicate declarations are reported as errors.
/// Returns `(flattened_classes, flattened_enums, errors)`.
pub fn flatten_classes(
    program: &Program,
) -> (
    Vec<FlattenedClass>,
    HashMap<String, FlattenedClass>,
    Vec<CompileError>,
) {
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
    let mut flattened_enums = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::ClassDecl {
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
            } => {
                if let Err(error) =
                    validation::validate_direct_members(properties, methods, stmt.span, name)
                {
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
                    used_traits: used_trait_names(trait_uses),
                    trait_aliases: used_trait_aliases(trait_uses, &trait_map),
                });
            }
            StmtKind::EnumDecl {
                name,
                implements,
                trait_uses,
                methods,
                constants,
                ..
            } => {
                if let Err(error) =
                    validation::validate_direct_members(&[], methods, stmt.span, name)
                {
                    errors.extend(error.flatten());
                    continue;
                }
                let (imported_props, imported_methods) = match expand::resolve_trait_uses(
                    trait_uses,
                    &trait_map,
                    &mut cache,
                    &mut stack,
                    &format!("enum {}", name),
                    stmt.span,
                ) {
                    Ok(result) => result,
                    Err(error) => {
                        errors.extend(error.flatten());
                        continue;
                    }
                };
                if let Some(property) = imported_props.first() {
                    errors.push(CompileError::new(
                        property.span,
                        "Enums cannot use traits with properties",
                    ));
                    continue;
                }
                let merged_methods = match merge::merge_methods(
                    imported_methods,
                    methods,
                    stmt.span,
                    &format!("enum {}", name),
                ) {
                    Ok(methods) => methods,
                    Err(error) => {
                        errors.extend(error.flatten());
                        continue;
                    }
                };
                let (_merged_props, merged_methods) =
                    crate::magic_constants::bind_trait_class_constants(
                        Vec::new(),
                        merged_methods,
                        name,
                    );
                flattened_enums.insert(
                    name.clone(),
                    FlattenedClass {
                        name: name.clone(),
                        extends: None,
                        implements: implements
                            .iter()
                            .map(|name| name.as_str().to_string())
                            .collect(),
                        is_abstract: false,
                        is_final: true,
                        is_readonly_class: false,
                        properties: Vec::new(),
                        methods: merged_methods,
                        attributes: stmt.attributes.clone(),
                        constants: constants.clone(),
                        used_traits: used_trait_names(trait_uses),
                        trait_aliases: used_trait_aliases(trait_uses, &trait_map),
                    },
                );
            }
            _ => {}
        }
    }

    (flattened, flattened_enums, errors)
}

/// Returns the direct trait names from a group of trait-use declarations.
fn used_trait_names(trait_uses: &[TraitUse]) -> Vec<String> {
    trait_uses
        .iter()
        .flat_map(|use_decl| {
            use_decl
                .trait_names
                .iter()
                .map(|name| name.as_str().to_string())
        })
        .collect()
}

/// Returns direct trait aliases in PHP's `alias => Trait::method` reflection format.
fn used_trait_aliases(
    trait_uses: &[TraitUse],
    trait_map: &HashMap<String, TraitDeclInfo>,
) -> Vec<(String, String)> {
    trait_uses
        .iter()
        .flat_map(|use_decl| {
            use_decl.adaptations.iter().filter_map(|adaptation| {
                let TraitAdaptation::Alias {
                    trait_name,
                    method,
                    alias: Some(alias),
                    ..
                } = adaptation
                else {
                    return None;
                };
                let source_trait = trait_name
                    .as_ref()
                    .map(|name| name.as_str().to_string())
                    .or_else(|| trait_alias_source_trait(use_decl, method, trait_map))?;
                Some((alias.clone(), format!("{source_trait}::{method}")))
            })
        })
        .collect()
}

/// Resolves the direct trait that supplies one unqualified alias adaptation target.
fn trait_alias_source_trait(
    trait_use: &TraitUse,
    method: &str,
    trait_map: &HashMap<String, TraitDeclInfo>,
) -> Option<String> {
    let method_key = php_symbol_key(method);
    trait_use.trait_names.iter().find_map(|trait_name| {
        let trait_info = trait_map.get(trait_name.as_str())?;
        trait_info
            .methods
            .iter()
            .any(|candidate| php_symbol_key(&candidate.name) == method_key)
            .then(|| trait_name.as_str().to_string())
    })
}
