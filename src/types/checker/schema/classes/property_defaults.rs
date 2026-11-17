//! Purpose:
//! Validates lexical receivers in trait properties and constructor-promoted defaults.
//!
//! Called from:
//! - `crate::types::checker::driver` before trait flattening.
//! - `super::properties::apply_properties` during class schema construction.
//!
//! Key details:
//! - Traits must reject late-static defaults even when they have no consumers.
//! - A trait's parent receiver remains unresolved until a consuming class binds its scope.
//! - For classes that use traits, it also records the class's own property order and the
//!   defaults php-src evaluates at the class statement to check a compatible redeclaration.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, ClassProperty, Expr, Program, StmtKind, TraitUse};
use crate::types::traits::FlattenedClass;
use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};

/// Retains source-member origins that flattening deliberately removes from its public schema.
pub(crate) fn collect_trait_default_origins(
    program: &Program,
    classes: &[FlattenedClass],
) -> (HashMap<String, HashSet<String>>, HashSet<String>) {
    let mut properties = HashMap::default();
    let mut constructors = HashSet::default();
    for statement in program {
        match &statement.kind {
            StmtKind::ClassDecl { name, properties: direct, methods, .. } => {
                let Some(class) = classes.iter().find(|class| class.name == *name) else { continue };
                if class.used_traits.is_empty() { continue; }
                properties.insert(name.clone(), class.properties.iter()
                    .filter(|property| !direct.iter().any(|local| local.name == property.name))
                    .map(|property| property.name.clone()).collect());
                if !methods.iter().any(|method| php_symbol_key(&method.name) == "__construct")
                    && class.methods.iter().any(|method| php_symbol_key(&method.name) == "__construct")
                {
                    constructors.insert(name.clone());
                }
            }
            StmtKind::NamespaceBlock { body, .. } => {
                let (nested_properties, nested_constructors) = collect_trait_default_origins(body, classes);
                properties.extend(nested_properties);
                constructors.extend(nested_constructors);
            }
            _ => {}
        }
    }
    (properties, constructors)
}

/// Property facts about classes that use traits, which flattening does not keep.
#[derive(Default)]
pub(crate) struct TraitPropertyOrigins {
    /// The class's own property names in source order, keyed by class.
    pub(crate) direct_order: HashMap<String, Vec<String>>,
    /// The default expressions php-src evaluates at a class or trait statement because one of
    /// its properties compatibly redeclares a used trait's property: for each such property,
    /// in trait order, the redeclaring default and then the trait's. Keyed by class or trait.
    pub(crate) collision_defaults: HashMap<String, Vec<crate::types::TraitCollisionDefault>>,
    /// How each class-like is bound, and where its imported properties come from.
    pub(crate) binding: crate::types::ClassBindingFacts,
}

/// Collects [`TraitPropertyOrigins`] for every class declaration that uses traits.
pub(crate) fn collect_trait_property_origins(program: &Program) -> TraitPropertyOrigins {
    let mut traits = HashMap::default();
    collect_trait_declarations(program, &mut traits);
    let mut origins = TraitPropertyOrigins::default();
    collect_class_trait_origins(program, &traits, &mut origins);
    let mut marked = HashSet::default();
    collect_class_binding_marks(program, &mut marked);
    collect_runtime_bound(program, true, &marked, &mut HashSet::default(), &mut origins.binding);
    origins
}

/// Collects the classes and traits a class-binding mark stands for (see
/// `crate::parser::ast::class_binding_mark_label`): those hoisted out of an included file,
/// wherever the include sits.
fn collect_class_binding_marks(program: &[crate::parser::ast::Stmt], out: &mut HashSet<String>) {
    for statement in program {
        match &statement.kind {
            StmtKind::IncludeOnceMark { source_path } => {
                // A class-binding mark carries its label in the path slot (see
                // `ir_lower::stmt::includes::lower_include_once_mark`).
                if let Some(name) = source_path.to_str().and_then(crate::parser::ast::class_binding_mark_class) {
                    out.insert(php_symbol_key(name));
                }
            }
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                collect_class_binding_marks(then_body, out);
                for (_, body) in elseif_clauses {
                    collect_class_binding_marks(body, out);
                }
                if let Some(body) = else_body {
                    collect_class_binding_marks(body, out);
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_class_binding_marks(body, out);
                }
                if let Some(body) = default {
                    collect_class_binding_marks(body, out);
                }
            }
            StmtKind::Try { try_body, catches, finally_body } => {
                collect_class_binding_marks(try_body, out);
                for clause in catches {
                    collect_class_binding_marks(&clause.body, out);
                }
                if let Some(body) = finally_body {
                    collect_class_binding_marks(body, out);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Foreach { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. }
            | StmtKind::NamespaceBlock { body, .. }
            | StmtKind::FunctionDecl { body, .. } => collect_class_binding_marks(body, out),
            StmtKind::ClassDecl { methods, .. } | StmtKind::TraitDecl { methods, .. } => {
                for method in methods {
                    collect_class_binding_marks(&method.body, out);
                }
            }
            _ => {}
        }
    }
}

/// Records which classes and traits php-src binds at their statement rather than at compile
/// time (CLI, no opcache: `zend_compile_class_decl`). A class binds at compile time when it is
/// top level, implements no interface, uses no trait, and has no parent or a parent already
/// bound at that point of the compilation (an internal class, or a user class bound earlier).
/// Interfaces and enums carry interfaces and are left out, as nothing is copied from them.
///
/// A class or trait hoisted out of an included file (`marked`) binds when the include runs, and
/// an anonymous class when its `new` first runs: both copy at that moment, so both are recorded
/// runtime-bound and bound elsewhere, and neither is bound while the root file compiles.
fn collect_runtime_bound(
    program: &[crate::parser::ast::Stmt],
    top_level: bool,
    marked: &HashSet<String>,
    bound: &mut HashSet<String>,
    binding: &mut crate::types::ClassBindingFacts,
) {
    for statement in program {
        match &statement.kind {
            StmtKind::ClassDecl { name, .. } | StmtKind::TraitDecl { name, .. }
                if marked.contains(&php_symbol_key(name.trim_start_matches('\\')))
                    || crate::parser::is_anonymous_class_name(name) =>
            {
                binding.bound_elsewhere.insert(php_symbol_key(name.trim_start_matches('\\')));
                binding.runtime_bound.insert(name.clone());
            }
            StmtKind::ClassDecl { name, extends, implements, trait_uses, .. } => {
                let runtime = &mut binding.runtime_bound;
                let parent_bound = extends.as_ref().is_none_or(|parent| {
                    let parent = parent.as_str().trim_start_matches('\\');
                    bound.contains(&php_symbol_key(parent)) || elephc_builtin_contract::lookup_class(parent).is_some()
                });
                if top_level && implements.is_empty() && trait_uses.is_empty() && parent_bound {
                    bound.insert(php_symbol_key(name));
                } else {
                    runtime.insert(name.clone());
                }
            }
            StmtKind::TraitDecl { name, trait_uses, .. } => {
                if top_level && trait_uses.is_empty() {
                    bound.insert(php_symbol_key(name));
                } else {
                    binding.runtime_bound.insert(name.clone());
                }
            }
            StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) | StmtKind::IncludeOnceGuard { body, .. } => {
                collect_runtime_bound(body, top_level, marked, bound, binding)
            }
            _ => {}
        }
        // A declaration inside a condition, a loop, a `try`, or a function or method body is
        // not top level: php-src binds it when its statement runs.
        for body in nested_statement_bodies(statement) {
            collect_runtime_bound(body, false, marked, bound, binding);
        }
    }
}

/// Returns the statement lists nested in `statement` whose declarations are not top level:
/// the branches of a condition, loop and `try` bodies, and function and method bodies.
fn nested_statement_bodies(statement: &crate::parser::ast::Stmt) -> Vec<&[crate::parser::ast::Stmt]> {
    match &statement.kind {
        StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
            let mut bodies: Vec<&[crate::parser::ast::Stmt]> = vec![then_body.as_slice()];
            bodies.extend(elseif_clauses.iter().map(|(_, body)| body.as_slice()));
            bodies.extend(else_body.as_deref());
            bodies
        }
        StmtKind::Switch { cases, default, .. } => {
            let mut bodies: Vec<&[crate::parser::ast::Stmt]> = cases.iter().map(|(_, body)| body.as_slice()).collect();
            bodies.extend(default.as_deref());
            bodies
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            let mut bodies: Vec<&[crate::parser::ast::Stmt]> = vec![try_body.as_slice()];
            bodies.extend(catches.iter().map(|clause| clause.body.as_slice()));
            bodies.extend(finally_body.as_deref());
            bodies
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::FunctionDecl { body, .. } => vec![body.as_slice()],
        StmtKind::ClassDecl { methods, .. } | StmtKind::TraitDecl { methods, .. } => {
            methods.iter().map(|method| method.body.as_slice()).collect()
        }
        _ => Vec::new(),
    }
}

/// Indexes every trait declaration by name: its own uses and its properties.
fn collect_trait_declarations<'a>(
    program: &'a Program,
    traits: &mut HashMap<String, (&'a [TraitUse], &'a [ClassProperty])>,
) {
    for statement in program {
        match &statement.kind {
            StmtKind::TraitDecl { name, trait_uses, properties, .. } => {
                traits.insert(php_symbol_key(name.trim_start_matches('\\')), (trait_uses.as_slice(), properties.as_slice()));
            }
            StmtKind::NamespaceBlock { body, .. } => collect_trait_declarations(body, traits),
            _ => {}
        }
    }
}

/// Appends a used trait's properties, depth first through the traits it uses itself.
fn trait_properties<'a>(
    name: &str,
    traits: &HashMap<String, (&'a [TraitUse], &'a [ClassProperty])>,
    seen: &mut HashSet<String>,
    out: &mut Vec<&'a ClassProperty>,
) {
    let key = php_symbol_key(name.trim_start_matches('\\'));
    if !seen.insert(key.clone()) {
        return;
    }
    let Some((uses, properties)) = traits.get(&key) else { return };
    out.extend(properties.iter());
    for used in uses.iter().flat_map(|trait_use| &trait_use.trait_names) {
        trait_properties(used.as_str(), traits, seen, out);
    }
}

/// Records the origins of each class declaration that uses traits, and the trait-collision
/// defaults of each trait declaration that uses traits itself.
fn collect_class_trait_origins(
    program: &Program,
    traits: &HashMap<String, (&[TraitUse], &[ClassProperty])>,
    origins: &mut TraitPropertyOrigins,
) {
    for statement in program {
        match &statement.kind {
            StmtKind::ClassDecl { name, trait_uses, properties: direct, .. } if !trait_uses.is_empty() => {
                origins.direct_order.insert(
                    name.clone(),
                    direct.iter().map(|property| property.name.clone()).collect(),
                );
                record_collision_defaults(name, trait_uses, direct, traits, origins);
            }
            StmtKind::TraitDecl { name, trait_uses, properties: direct, .. } if !trait_uses.is_empty() => {
                record_collision_defaults(name, trait_uses, direct, traits, origins);
            }
            StmtKind::NamespaceBlock { body, .. } => collect_class_trait_origins(body, traits, origins),
            _ => {}
        }
    }
}

/// Records the defaults php-src evaluates when it binds the traits `uses` into the class or
/// trait `name`: binding the traits in `use` order, each imported property whose name the
/// class already holds (declared by it, or imported from an earlier trait) is compared with
/// the held one, evaluating the held default and then the imported one. A used trait's property
/// already holds what the traits it uses itself contributed, so each used trait contributes a
/// name once.
fn record_collision_defaults(
    name: &str,
    uses: &[TraitUse],
    direct: &[ClassProperty],
    traits: &HashMap<String, (&[TraitUse], &[ClassProperty])>,
    origins: &mut TraitPropertyOrigins,
) {
    use crate::types::TraitCollisionDefault;
    let mut held: Vec<(&str, Option<&Expr>, Option<&str>)> =
        direct.iter().map(|property| (property.name.as_str(), property.default.as_ref(), None)).collect();
    let mut collisions = Vec::new();
    let mut property_origins = HashMap::default();
    for used in uses.iter().flat_map(|trait_use| &trait_use.trait_names) {
        let mut imported = Vec::new();
        trait_properties(used.as_str(), traits, &mut HashSet::default(), &mut imported);
        let mut contributed = HashSet::default();
        for trait_property in imported {
            if !contributed.insert(trait_property.name.as_str()) {
                continue;
            }
            match held.iter().find(|(held_name, _, _)| *held_name == trait_property.name) {
                Some((_, held_default, held_trait)) => {
                    if let (Some(held_default), Some(trait_default)) = (held_default, &trait_property.default) {
                        collisions.push(TraitCollisionDefault {
                            default: (*held_default).clone(),
                            trait_name: held_trait.map(str::to_string),
                        });
                        collisions.push(TraitCollisionDefault {
                            default: trait_default.clone(),
                            trait_name: Some(used.as_str().to_string()),
                        });
                    }
                }
                None => {
                    property_origins.insert(trait_property.name.clone(), used.as_str().trim_start_matches('\\').to_string());
                    held.push((trait_property.name.as_str(), trait_property.default.as_ref(), Some(used.as_str())));
                }
            }
        }
    }
    if !collisions.is_empty() {
        origins.collision_defaults.insert(name.to_string(), collisions);
    }
    if !property_origins.is_empty() {
        origins.binding.property_trait_origins.insert(name.to_string(), property_origins);
    }
    origins.binding.trait_uses.insert(
        name.to_string(),
        uses.iter()
            .flat_map(|trait_use| &trait_use.trait_names)
            .map(|used| used.as_str().trim_start_matches('\\').to_string())
            .collect(),
    );
}

/// Validates defaults on every original trait declaration, including unused declarations.
pub(crate) fn validate_trait_property_defaults(program: &Program) -> Vec<CompileError> {
    let mut errors = Vec::new();
    for statement in program {
        match &statement.kind {
            StmtKind::TraitDecl { name, properties, methods, .. } => {
                // This parent is only a validation placeholder; the rewritten tree is discarded.
                // Traits have no parent of their own, so binding it here would reject valid PHP.
                for property in properties {
                    if let Some(default) = &property.default {
                        if let Err(error) = super::constants::validate_property_default_in_scope(
                            default, name, Some(name),
                        ) {
                            errors.extend(error.flatten());
                        }
                    }
                }
                if let Err(error) = validate_promoted_defaults(properties, methods, name, Some(name)) {
                    errors.extend(error.flatten());
                }
            }
            StmtKind::NamespaceBlock { body, .. } => {
                errors.extend(validate_trait_property_defaults(body));
            }
            _ => {}
        }
    }
    errors
}

/// Checks promoted initializers stored on constructor parameters instead of property defaults.
pub(super) fn validate_promoted_defaults(
    properties: &[ClassProperty],
    methods: &[ClassMethod],
    class_name: &str,
    parent_name: Option<&str>,
) -> Result<(), CompileError> {
    for method in methods.iter().filter(|method| php_symbol_key(&method.name) == "__construct") {
        for (name, _, default, _) in &method.params {
            if !properties.iter().any(|property| property.is_promoted && property.name == *name) {
                continue;
            }
            if let Some(default) = default {
                super::constants::validate_compile_time_default_in_scope(
                    default, class_name, parent_name,
                )?;
                if let Err(error) = super::constants::validate_property_default_in_scope(
                    default, class_name, parent_name,
                ) {
                    if error.message != "Cannot access \"parent\" when current class scope has no parent" {
                        return Err(error);
                    }
                }
            }
        }
    }
    Ok(())
}
