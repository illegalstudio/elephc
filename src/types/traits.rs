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
    Visibility,
};
use crate::span::Span;

mod expand;
mod merge;
mod validation;

#[derive(Debug, Clone)]
/// A class with all traits fully expanded, property/method/constant conflicts resolved,
/// and direct members merged. Produced by `flatten_classes` and consumed by
/// schema building, type checking, and codegen.
pub struct FlattenedClass {
    pub name: String,
    pub span: Span,
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
    constants: Vec<ClassConst>,
    span: Span,
}

#[derive(Clone)]
/// Cached result of fully expanding a trait: all properties, methods, and constants
/// after applying trait_uses, conflict resolution, and adaptations.
/// Stored in the expansion cache to avoid repeated work.
struct ExpandedTrait {
    properties: Vec<ClassProperty>,
    methods: Vec<TraitMethod>,
    constants: Vec<ClassConst>,
}

#[derive(Clone)]
/// A method of an expanded trait with the trait that declares it.
///
/// `origin` names the declaring trait after generic instantiation (`Holder<int>` and
/// `Holder<string>` are two traits), so two copies of one method reached along two `use`
/// paths share it, while two instantiations of one generic method, which share a span, do not.
struct TraitMethod {
    origin: String,
    decl: ClassMethod,
}

#[derive(Clone)]
/// A trait method imported during trait composition, with its source trait
/// tracked for insteadof conflict resolution and visibility override resolution.
///
/// `binding` orders the methods as PHP binds them (`zend_do_traits_method_binding`): by used
/// trait, then by method within it, an alias (numbered in declaration order) just before the
/// method it copies (`usize::MAX`).
struct ImportedMethod {
    source_trait: String,
    origin: String,
    decl: ClassMethod,
    binding: (usize, usize, usize),
}

/// Scans `program` for all traits, classes, and enums, validates direct member uniqueness,
/// expands trait uses for each class-like declaration, and returns flattened metadata with
/// any composition errors collected.
///
/// Trait declarations are stored in `trait_map` for later expansion.
/// Class-like declarations with traits are processed in program order; each declaration's trait
/// uses are resolved recursively, then merged with the declaration's own members.
/// Circular trait composition and duplicate declarations are reported as errors.
/// Returns `(flattened_classes, flattened_enums, errors, final_abstract_imports)`; the last are
/// checked by `final_abstract_import_overrides` once builtin classes are registered too.
pub fn flatten_classes(
    program: &Program,
) -> (
    Vec<FlattenedClass>,
    HashMap<String, FlattenedClass>,
    Vec<CompileError>,
    Vec<FinalAbstractImport>,
) {
    let mut trait_map = HashMap::new();
    let mut trait_keys = HashSet::new();
    let mut class_like_keys = HashSet::new();
    let mut errors = Vec::new();
    let mut final_abstract_imports = Vec::new();

    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                // Always `None` here: `crate::generics::classes` strips every trait template
                // before the checker runs, and each instantiation is an ordinary trait.
                generics: _,
                trait_uses,
                properties,
                methods,
                constants,
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
                        constants: constants.clone(),
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
                generics: _,
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
                let (imported_props, imported_methods, imported_constants, bound_methods) =
                    match expand::resolve_trait_uses(
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
                // Ancestors, builtin ones included, are only known to the checker driver, so a
                // class's abstract `as final` imports are checked there.
                final_abstract_imports.extend(final_import_groups(bound_methods).into_iter().map(
                    |imports| FinalAbstractImport {
                        class: name.clone(),
                        own: own_method_span(methods, &imports[0].decl.name),
                        span: stmt.span,
                        imports,
                    },
                ));
                let merged_methods = match merge::merge_methods(
                    imported_methods,
                    methods,
                    name,
                    stmt.span,
                    &format!("class {}", name),
                ) {
                    Ok(methods) => methods.into_iter().map(|method| method.decl).collect(),
                    Err(error) => {
                        errors.extend(error.flatten());
                        continue;
                    }
                };
                let imported_constants = crate::magic_constants::bind_trait_constant_values(
                    imported_constants, name,
                );
                let merged_constants = match merge::merge_constants(
                    &imported_constants,
                    constants,
                    stmt.span,
                    &format!("class {}", name),
                ) {
                    Ok(constants) => constants,
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
                    span: stmt.span,
                    extends: extends.as_ref().map(|name| name.as_str().to_string()),
                    implements: implements.iter().map(|name| name.as_str().to_string()).collect(),
                    is_abstract: *is_abstract,
                    is_final: *is_final,
                    is_readonly_class: *is_readonly_class,
                    properties: merged_props,
                    methods: merged_methods,
                    attributes: stmt.attributes.clone(),
                    constants: merged_constants,
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
                let (imported_props, imported_methods, imported_constants, bound_methods) =
                    match expand::resolve_trait_uses(
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
                if let Err(error) = reject_overridden_final_imports(
                    &final_import_groups(bound_methods),
                    methods,
                    stmt.span,
                ) {
                    errors.extend(error.flatten());
                    continue;
                }
                let merged_methods = match merge::merge_methods(
                    imported_methods,
                    methods,
                    name,
                    stmt.span,
                    &format!("enum {}", name),
                ) {
                    Ok(methods) => methods.into_iter().map(|method| method.decl).collect(),
                    Err(error) => {
                        errors.extend(error.flatten());
                        continue;
                    }
                };
                let imported_constants = crate::magic_constants::bind_trait_constant_values(
                    imported_constants, name,
                );
                let merged_constants = match merge::merge_constants(
                    &imported_constants,
                    constants,
                    stmt.span,
                    &format!("enum {}", name),
                ) {
                    Ok(constants) => constants,
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
                        methods: merged_methods,
                        attributes: stmt.attributes.clone(),
                        constants: merged_constants,
                        used_traits: used_trait_names(trait_uses),
                        trait_aliases: used_trait_aliases(trait_uses, &trait_map),
                    },
                );
            }
            _ => {}
        }
    }

    (flattened, flattened_enums, errors, final_abstract_imports)
}

/// The methods a class's trait uses bind under one name that an abstract `as final` import
/// takes part in, in binding order, recorded while flattening the class and checked by
/// `final_abstract_import_overrides` once the class's ancestors are known. `own` is the span and
/// kind (static or not) of the class's own method of that name, and `span` the class
/// declaration.
pub struct FinalAbstractImport {
    class: String,
    own: Option<(Span, bool)>,
    span: Span,
    imports: Vec<ImportedMethod>,
}

/// The class-body or concrete imported methods that are checked as overrides of a visible
/// ancestor method before an abstract `as final` import they replace is reported, by lowercase
/// class and method name, with the importing trait and the import's method name. The class
/// schema reports `Cannot override final method T::foo()` for one only after its ordinary
/// override checks pass, as PHP inherits the parent before it binds abstract trait methods.
pub(crate) type FinalAbstractBodyOverrides = HashMap<(String, String), (String, String)>;

/// Groups `bound` (in binding order) by method name, keeping the groups an abstract `as final`
/// import takes part in.
fn final_import_groups(bound: Vec<ImportedMethod>) -> Vec<Vec<ImportedMethod>> {
    let mut groups: Vec<(String, Vec<ImportedMethod>)> = Vec::new();
    for method in bound {
        let key = php_symbol_key(&method.decl.name);
        match groups.iter_mut().find(|(name, _)| *name == key) {
            Some((_, group)) => group.push(method),
            None => groups.push((key, vec![method])),
        }
    }
    groups
        .into_iter()
        .map(|(_, group)| group)
        .filter(|group| group.iter().any(|method| method.decl.is_abstract && method.decl.is_final))
        .collect()
}

/// Returns the span and kind (static or not) of the method named `name` among `methods`.
fn own_method_span(methods: &[ClassMethod], name: &str) -> Option<(Span, bool)> {
    methods
        .iter()
        .find(|method| method.name.eq_ignore_ascii_case(name))
        .map(|method| (method.span, method.is_static))
}

/// Returns the first concrete method among `imports`, the one PHP binds before the abstract
/// ones (two concrete imports are a collision reported while selecting methods).
fn concrete_import(imports: &[ImportedMethod]) -> Option<&ImportedMethod> {
    imports.iter().find(|method| !method.decl.is_abstract)
}

/// Replays how Zend binds one method name (`zend_do_link_class`, `zend_add_trait_method`) and
/// returns the index in `imports` of the first abstract `as final` import that another method
/// overrides. Concrete trait methods bind first, where the class-like's own method wins, then
/// the parent's methods are inherited, then the abstract trait methods bind in order, each
/// checked as the parent of whatever already holds the name: a final one fails first, and a
/// plain one of the other kind (static or instance) fails the kind check, which the class schema
/// reports, so no final import is reported after it. A later copy of the abstract trait method
/// already bound is skipped. `held` is whether the holder before the abstract ones bind (an own
/// method, a concrete import or an ancestor method) is static, or `None` without one.
fn first_overridden_final_import(held: Option<bool>, imports: &[ImportedMethod]) -> Option<usize> {
    let mut holder_is_static = held;
    let mut bound: Option<&ImportedMethod> = None;
    for (index, method) in imports.iter().enumerate() {
        if !method.decl.is_abstract {
            continue;
        }
        let Some(is_static) = holder_is_static else {
            holder_is_static = Some(method.decl.is_static);
            bound = Some(method);
            continue;
        };
        if bound.is_some_and(|existing| {
            merge::is_same_trait_method(
                (&existing.origin, &existing.decl),
                (&method.origin, &method.decl),
            )
        }) {
            continue;
        }
        if method.decl.is_final {
            return Some(index);
        }
        if method.decl.is_static != is_static {
            return None;
        }
    }
    None
}

/// The error for `import`, an abstract `as final` trait method that another method overrides.
fn final_import_override(span: Span, import: &ImportedMethod) -> CompileError {
    CompileError::new(
        span,
        &format!(
            "Cannot override final method {}::{}",
            import.source_trait, import.decl.name
        ),
    )
}

/// Reports the first abstract `as final` import of a trait or enum that its own method, a
/// concrete import or an earlier abstract import overrides (`first_overridden_final_import`).
/// Traits and enums have no parent, so nothing is inherited before the abstract methods bind.
fn reject_overridden_final_imports(
    groups: &[Vec<ImportedMethod>],
    own_methods: &[ClassMethod],
    span: Span,
) -> Result<(), CompileError> {
    for imports in groups {
        let own = own_methods
            .iter()
            .find(|method| method.name.eq_ignore_ascii_case(&imports[0].decl.name));
        let held = own
            .map(|method| method.is_static)
            .or_else(|| concrete_import(imports).map(|method| method.decl.is_static));
        if let Some(index) = first_overridden_final_import(held, imports) {
            let at = own.map_or(span, |method| method.span);
            return Err(final_import_override(at, &imports[index]));
        }
    }
    Ok(())
}

/// Checks each class's abstract `as final` imports once every class, builtin ones included, is
/// in `classes` (`first_overridden_final_import`, with the nearest ancestor method of the name,
/// private or static included, holding the name too). An overridden import is reported, and
/// every method of the name is dropped so the schema reports nothing more about it, except when
/// the method holding the name is the class's own or a concrete import and a visible ancestor
/// method declares it too: PHP checks that override first, so the method stays, the others of
/// the name are dropped, and it is returned in the body-override map for the class schema.
pub(crate) fn final_abstract_import_overrides(
    groups: &[FinalAbstractImport],
    classes: &mut HashMap<String, FlattenedClass>,
) -> (Vec<CompileError>, FinalAbstractBodyOverrides) {
    let by_key: HashMap<String, &FlattenedClass> = classes
        .values()
        .map(|class| (php_symbol_key(&class.name), class))
        .collect();
    let mut errors = Vec::new();
    let mut body_overrides = FinalAbstractBodyOverrides::new();
    // Class, method name, and the span of the method that stays, if any.
    let mut dropped: Vec<(&str, &str, Option<Span>)> = Vec::new();
    for group in groups {
        let name = &group.imports[0].decl.name;
        let ancestor = nearest_ancestor_method(&by_key, &group.class, name);
        let concrete = concrete_import(&group.imports);
        let own = group.own.map(|(span, _)| span);
        let held = group
            .own
            .map(|(_, is_static)| is_static)
            .or_else(|| concrete.map(|method| method.decl.is_static))
            .or_else(|| ancestor.map(|method| method.is_static));
        let Some(index) = first_overridden_final_import(held, &group.imports) else {
            continue;
        };
        let import = &group.imports[index];
        let holder = own.or(concrete.map(|method| method.decl.span));
        let checked_first =
            ancestor.is_some_and(|method| method.visibility != Visibility::Private);
        match holder {
            Some(holder) if checked_first => {
                body_overrides.insert(
                    (php_symbol_key(&group.class), php_symbol_key(name)),
                    (import.source_trait.clone(), import.decl.name.clone()),
                );
                dropped.push((&group.class, name, Some(holder)));
            }
            _ => {
                errors.push(final_import_override(own.unwrap_or(group.span), import));
                dropped.push((&group.class, name, None));
            }
        }
    }
    for (class_name, name, kept) in dropped {
        if let Some(class) = classes
            .values_mut()
            .find(|class| class.name.eq_ignore_ascii_case(class_name))
        {
            class.methods.retain(|method| {
                !method.name.eq_ignore_ascii_case(name) || Some(method.span) == kept
            });
        }
    }
    (errors, body_overrides)
}

/// Returns the method named `method` of the nearest flattened ancestor of `class` that declares
/// it (inherited trait methods included), or `None` when no user-declared ancestor does.
fn nearest_ancestor_method<'a>(
    classes: &HashMap<String, &'a FlattenedClass>,
    class: &str,
    method: &str,
) -> Option<&'a ClassMethod> {
    let mut visited = HashSet::new();
    let mut parent = classes
        .get(&php_symbol_key(class))
        .and_then(|class| class.extends.as_deref());
    while let Some(parent_name) = parent {
        let parent_key = php_symbol_key(parent_name.trim_start_matches('\\'));
        if !visited.insert(parent_key.clone()) {
            return None;
        }
        let parent_class = classes.get(&parent_key)?;
        if let Some(found) = parent_class
            .methods
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(method))
        {
            return Some(found);
        }
        parent = parent_class.extends.as_deref();
    }
    None
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
