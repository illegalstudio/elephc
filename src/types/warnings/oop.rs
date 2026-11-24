//! Purpose:
//! Emits checker warnings for oop cases.
//! Scans typed AST and checker metadata for suspicious but non-fatal program patterns.
//!
//! Called from:
//! - `crate::types::warnings`
//!
//! Key details:
//! - Warning analysis should preserve source spans and avoid rejecting programs that type checking accepted.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileWarning;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Stmt, StmtKind, TraitAdaptation, TraitUse, Visibility};

/// The methods and nested trait uses of each declared trait, by lowercase name.
type TraitTable<'a> = HashMap<String, (&'a [ClassMethod], &'a [TraitUse])>;

/// Collects the OOP-related warnings of a program.
///
/// Indexes every trait declaration first, so a `use` adaptation can look up the trait method it
/// makes final, then walks the statements with `collect_oop_stmt_warnings`.
pub(super) fn collect_oop_warnings(stmts: &[Stmt], warnings: &mut Vec<CompileWarning>) {
    let mut traits = TraitTable::new();
    index_traits(stmts, &mut traits);
    collect_oop_stmt_warnings(stmts, &traits, warnings);
}

/// Records each trait declared in `stmts`, namespace blocks and include guards included.
fn index_traits<'a>(stmts: &'a [Stmt], traits: &mut TraitTable<'a>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::TraitDecl { name, methods, trait_uses, .. } => {
                traits.insert(trait_key(name), (methods, trait_uses));
            }
            StmtKind::NamespaceBlock { body, .. } | StmtKind::IncludeOnceGuard { body, .. } => {
                index_traits(body, traits)
            }
            _ => {}
        }
    }
}

/// Recursively walks statements to collect OOP-related warnings.
///
/// Visits `ClassDecl`, `TraitDecl`, `EnumDecl`, `NamespaceBlock`, and `IncludeOnceGuard` nodes,
/// delegating modifier validation to `collect_method_modifier_warnings` for each class/trait
/// and the `as final` adaptations to `collect_final_adaptation_warnings`.
fn collect_oop_stmt_warnings(stmts: &[Stmt], traits: &TraitTable, warnings: &mut Vec<CompileWarning>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::ClassDecl { methods, trait_uses, .. }
            | StmtKind::TraitDecl { methods, trait_uses, .. } => {
                collect_method_modifier_warnings(methods, warnings);
                collect_final_adaptation_warnings(trait_uses, traits, warnings);
            }
            StmtKind::EnumDecl { trait_uses, .. } => {
                collect_final_adaptation_warnings(trait_uses, traits, warnings);
            }
            StmtKind::NamespaceBlock { body, .. } | StmtKind::IncludeOnceGuard { body, .. } => {
                collect_oop_stmt_warnings(body, traits, warnings)
            }
            _ => {}
        }
    }
}

/// Warns, as PHP does, when an `as final` adaptation (with or without an alias) makes a trait
/// method final that is private in the trait, as declared or as a nested `use` adapted it,
/// since a private method is never overridden.
/// The warning follows the trait method's own visibility: `foo as private; foo as final` on a
/// public trait method does not warn. Constructors are exempt, as for a written `final private`.
fn collect_final_adaptation_warnings(
    trait_uses: &[TraitUse],
    traits: &TraitTable,
    warnings: &mut Vec<CompileWarning>,
) {
    for trait_use in trait_uses {
        for adaptation in &trait_use.adaptations {
            let TraitAdaptation::Alias { trait_name, method, is_final: true, .. } = adaptation
            else {
                continue;
            };
            if method.eq_ignore_ascii_case("__construct") {
                continue;
            }
            let candidates: Vec<&str> = match trait_name {
                Some(trait_name) => vec![trait_name.as_str()],
                None => trait_use.trait_names.iter().map(|name| name.as_str()).collect(),
            };
            let private = candidates.into_iter().any(|trait_name| {
                trait_method_visibility(trait_name, method, traits, &mut HashSet::new())
                    == Some(Visibility::Private)
            });
            if private {
                warnings.push(CompileWarning::new(
                    trait_use.span,
                    "Private methods cannot be final as they are never overridden by other classes",
                ));
            }
        }
    }
}

/// Returns the visibility `method` has in trait `trait_name` as PHP composes it: the trait's own
/// declaration, otherwise the method one of its `use` clauses brings in (`use_method_visibility`),
/// or `None` when no known trait provides it.
fn trait_method_visibility(
    trait_name: &str,
    method: &str,
    traits: &TraitTable,
    visited: &mut HashSet<(String, String)>,
) -> Option<Visibility> {
    let key = trait_key(trait_name);
    if !visited.insert((key.clone(), php_symbol_key(method))) {
        return None;
    }
    let (methods, trait_uses) = traits.get(&key)?;
    if let Some(declared) = methods.iter().find(|m| m.name.eq_ignore_ascii_case(method)) {
        return Some(declared.visibility.clone());
    }
    trait_uses
        .iter()
        .find_map(|trait_use| use_method_visibility(trait_use, method, traits, visited))
}

/// Returns the visibility of `method` as imported by one nested `use` clause: an alias named
/// `method` takes its adaptation's visibility or the aliased method's, and a method imported
/// under its own name takes the last visibility change (`foo as private;`) or its own.
fn use_method_visibility(
    trait_use: &TraitUse,
    method: &str,
    traits: &TraitTable,
    visited: &mut HashSet<(String, String)>,
) -> Option<Visibility> {
    let mut source_visibility = |source_trait: &Option<crate::names::Name>, source: &str| {
        let candidates: Vec<&str> = match source_trait {
            Some(source_trait) => vec![source_trait.as_str()],
            None => trait_use.trait_names.iter().map(|name| name.as_str()).collect(),
        };
        candidates
            .into_iter()
            .find_map(|candidate| trait_method_visibility(candidate, source, traits, visited))
    };
    for adaptation in &trait_use.adaptations {
        if let TraitAdaptation::Alias { trait_name, method: source, alias: Some(alias), visibility, .. } =
            adaptation
        {
            if alias.eq_ignore_ascii_case(method) {
                if let Some(original) = source_visibility(trait_name, source) {
                    return Some(visibility.clone().unwrap_or(original));
                }
            }
        }
    }
    let original = source_visibility(&None, method)?;
    let changed = trait_use.adaptations.iter().rev().find_map(|adaptation| match adaptation {
        TraitAdaptation::Alias { method: target, alias: None, visibility: Some(visibility), .. }
            if target.eq_ignore_ascii_case(method) =>
        {
            Some(visibility.clone())
        }
        _ => None,
    });
    Some(changed.unwrap_or(original))
}

/// The lookup key of a trait name: case-insensitive, without a leading namespace separator.
fn trait_key(name: &str) -> String {
    php_symbol_key(name.trim_start_matches('\\'))
}

/// Checks each class/trait method for invalid modifier combinations and emits warnings.
///
/// Currently warns when a non-constructor private method is marked `final`, since private
/// methods cannot be overridden and the `final` qualifier has no effect in that context.
fn collect_method_modifier_warnings(methods: &[ClassMethod], warnings: &mut Vec<CompileWarning>) {
    for method in methods {
        if method.is_final
            && method.visibility == Visibility::Private
            && !method.name.eq_ignore_ascii_case("__construct")
        {
            warnings.push(CompileWarning::new(
                method.span,
                "Private methods cannot be final as they are never overridden by other classes",
            ));
        }
    }
}
