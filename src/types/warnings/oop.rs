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
/// method final that the trait declares private, since a private method is never overridden.
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

/// Returns the declared visibility of `method` in trait `trait_name`, looking through the
/// traits it uses when it does not declare the method itself, or `None` when no known trait
/// declares it.
fn trait_method_visibility(
    trait_name: &str,
    method: &str,
    traits: &TraitTable,
    visited: &mut HashSet<String>,
) -> Option<Visibility> {
    let key = trait_key(trait_name);
    if !visited.insert(key.clone()) {
        return None;
    }
    let (methods, trait_uses) = traits.get(&key)?;
    if let Some(declared) = methods.iter().find(|m| m.name.eq_ignore_ascii_case(method)) {
        return Some(declared.visibility.clone());
    }
    trait_uses
        .iter()
        .flat_map(|trait_use| trait_use.trait_names.iter())
        .find_map(|nested| trait_method_visibility(nested.as_str(), method, traits, visited))
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
