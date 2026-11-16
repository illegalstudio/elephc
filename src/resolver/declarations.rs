//! Purpose:
//! Extracts and strips declarations that can be discovered from included files.
//! Separates declaration availability from runtime execution of include statements.
//!
//! Called from:
//! - `crate::resolver::discovery` and include resolution paths.
//!
//! Key details:
//! - Discoverable declarations must keep namespace context and include-loaded function variant metadata.
//! - A class or trait hoisted out of an included file leaves a class-binding mark where php-src
//!   binds it, so its binding effects run when the include does.

use std::collections::HashSet;
use std::path::Path;

use crate::names::canonical_name_for_decl;
use crate::parser::ast::{class_binding_mark_label, Stmt, StmtKind};

use super::discovery::{FunctionVariantKey, FunctionVariantRegistry};
use super::state::namespace_string;

/// Recursively extracts top-level and namespace-scoped declarations that can be
/// discovered from included files, preserving their namespace and use contexts.
/// Returns declarations wrapped in NamespaceBlock or Synthetic nodes to retain scoping.
pub(super) fn extract_discoverable_declarations(stmts: &[Stmt]) -> Vec<Stmt> {
    let mut declarations = Vec::new();
    let mut context = Vec::new();
    let mut context_flushed = false;

    for stmt in stmts {
        let _source_mode = crate::source::scoped_parse_mode(stmt.profile());
        match &stmt.kind {
            StmtKind::NamespaceDecl { .. } => {
                context.clear();
                context.push(stmt.clone());
                context_flushed = false;
            }
            StmtKind::UseDecl { .. } => {
                context.push(stmt.clone());
                context_flushed = false;
            }
            StmtKind::NamespaceBlock { name, body } => {
                let body_declarations = extract_discoverable_declarations(body);
                if !body_declarations.is_empty() {
                    declarations.push(Stmt::new(
                        StmtKind::NamespaceBlock {
                            name: name.clone(),
                            body: body_declarations,
                        },
                        stmt.span,
                    ));
                }
            }
            StmtKind::Synthetic(body) => {
                let body_declarations = extract_discoverable_declarations(body);
                if !body_declarations.is_empty() {
                    if !context_flushed {
                        declarations.extend(context.clone());
                        context_flushed = true;
                    }
                    declarations.push(Stmt::new(StmtKind::Synthetic(body_declarations), stmt.span));
                }
            }
            kind if is_discoverable_declaration(kind) => {
                if !context_flushed {
                    declarations.extend(context.clone());
                    context_flushed = true;
                }
                declarations.push(stmt.clone());
            }
            _ => {}
        }
    }

    declarations
}

/// Removes discoverable declarations from the statement list, replacing function
/// declarations with FunctionVariantMark nodes that record the include-loaded variant.
/// Uses `canonical` path and `function_variants` registry to determine which variant
/// should be active in the including file's scope.
///
/// In an included file (`canonical` set) each named class or trait leaves a class-binding
/// mark (`class_binding_mark_label`) where php-src binds it: at the start of the file for one
/// it binds while compiling the file, at its statement for the others.
pub(super) fn strip_discoverable_declarations(
    stmts: Vec<Stmt>,
    canonical: Option<&Path>,
    function_variants: &FunctionVariantRegistry,
) -> Vec<Stmt> {
    if canonical.is_none() {
        return strip_stmts(stmts, canonical, function_variants, None, &HashSet::new());
    }
    let early = compile_time_bound_classes(&stmts);
    let early_keys: HashSet<String> = early.iter().map(|(name, _)| name.to_ascii_lowercase()).collect();
    let mut executable: Vec<Stmt> = early
        .into_iter()
        .map(|(name, span)| Stmt::new(StmtKind::IncludeOnceMark { label: class_binding_mark_label(&name) }, span))
        .collect();
    executable.extend(strip_stmts(stmts, canonical, function_variants, None, &early_keys));
    executable
}

/// Returns, in order, the named classes and traits of one file that php-src binds while
/// compiling it (CLI, no opcache): top level, no interface, no trait use, and a parent that is
/// declared outside the file (bound before the file is included) or bound earlier in it.
/// Each comes with its declaration span.
fn compile_time_bound_classes(stmts: &[Stmt]) -> Vec<(String, crate::span::Span)> {
    let mut declared = HashSet::new();
    collect_file_class_names(stmts, None, &mut declared);
    let mut bound = HashSet::new();
    let mut early = Vec::new();
    collect_compile_time_bound(stmts, None, &declared, &mut bound, &mut early);
    early
}

/// Collects the lowercased fully qualified names of the classes a file declares at top level.
fn collect_file_class_names(stmts: &[Stmt], namespace: Option<String>, out: &mut HashSet<String>) {
    let mut namespace = namespace;
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::NamespaceDecl { name } => namespace = Some(namespace_string(name)),
            StmtKind::NamespaceBlock { name, body } => collect_file_class_names(body, Some(namespace_string(name)), out),
            StmtKind::Synthetic(body) => collect_file_class_names(body, namespace.clone(), out),
            StmtKind::ClassDecl { name, .. } | StmtKind::TraitDecl { name, .. } => {
                out.insert(canonical_name_for_decl(namespace.as_deref(), name).to_ascii_lowercase());
            }
            _ => {}
        }
    }
}

/// Body of [`compile_time_bound_classes`].
fn collect_compile_time_bound(
    stmts: &[Stmt],
    namespace: Option<String>,
    declared: &HashSet<String>,
    bound: &mut HashSet<String>,
    early: &mut Vec<(String, crate::span::Span)>,
) {
    let mut namespace = namespace;
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::NamespaceDecl { name } => namespace = Some(namespace_string(name)),
            StmtKind::NamespaceBlock { name, body } => {
                collect_compile_time_bound(body, Some(namespace_string(name)), declared, bound, early)
            }
            StmtKind::Synthetic(body) => collect_compile_time_bound(body, namespace.clone(), declared, bound, early),
            StmtKind::ClassDecl { name, extends, implements, trait_uses, .. } if !crate::parser::is_anonymous_class_name(name) => {
                let parent_bound = extends.as_ref().is_none_or(|parent| {
                    let parent = if parent.is_fully_qualified() {
                        parent.as_str().trim_start_matches('\\').to_string()
                    } else {
                        canonical_name_for_decl(namespace.as_deref(), parent.as_str())
                    };
                    let parent = parent.to_ascii_lowercase();
                    !declared.contains(&parent) || bound.contains(&parent)
                });
                if implements.is_empty() && trait_uses.is_empty() && parent_bound {
                    let full = canonical_name_for_decl(namespace.as_deref(), name);
                    bound.insert(full.to_ascii_lowercase());
                    early.push((full, stmt.span));
                }
            }
            StmtKind::TraitDecl { name, trait_uses, .. } if trait_uses.is_empty() => {
                let full = canonical_name_for_decl(namespace.as_deref(), name);
                bound.insert(full.to_ascii_lowercase());
                early.push((full, stmt.span));
            }
            _ => {}
        }
    }
}

/// Internal recursive helper that processes statements and strips discoverable
/// declarations, tracking the current namespace context via `current_namespace`.
/// The `namespace` parameter carries the effective namespace for the current block.
fn strip_stmts(
    stmts: Vec<Stmt>,
    canonical: Option<&Path>,
    function_variants: &FunctionVariantRegistry,
    namespace: Option<String>,
    early: &HashSet<String>,
) -> Vec<Stmt> {
    let mut stripped = Vec::new();
    let mut namespace = namespace;
    for stmt in stmts {
        let _source_mode = crate::source::scoped_parse_mode(stmt.profile());
        let stmt_namespace = namespace.clone();
        if let Some(stmt) = strip_stmt(
            stmt,
            canonical,
            function_variants,
            stmt_namespace.as_deref(),
            &mut namespace,
            early,
        ) {
            stripped.push(stmt);
        }
    }
    stripped
}

/// Processes a single statement, removing discoverable declarations while
/// preserving namespace declarations and blocks. Function declarations are
/// replaced with FunctionVariantMark using the canonical path and registry to
/// resolve the correct variant. Updates `current_namespace` when entering a
/// NamespaceDecl.
fn strip_stmt(
    stmt: Stmt,
    canonical: Option<&Path>,
    function_variants: &FunctionVariantRegistry,
    namespace: Option<&str>,
    current_namespace: &mut Option<String>,
    early: &HashSet<String>,
) -> Option<Stmt> {
    let span = stmt.span;
    match stmt.kind {
        StmtKind::FunctionDecl { name, .. } => {
            let public_name = canonical_name_for_decl(namespace, &name);
            canonical
                .and_then(|canonical| {
                    function_variants.get(&FunctionVariantKey::new(
                        canonical,
                        &public_name,
                    ))
                })
                .map(|variant| {
                    Stmt::new(
                        StmtKind::FunctionVariantMark {
                            name: variant.public_name.clone(),
                            variant: variant.variant_name.clone(),
                        },
                        span,
                    )
                })
        }
        // A class or trait of an included file binds when the include runs: a mark keeps the
        // statement's place for one php-src binds there (see `strip_discoverable_declarations`).
        StmtKind::ClassDecl { name, .. } | StmtKind::TraitDecl { name, .. }
            if canonical.is_some() && !crate::parser::is_anonymous_class_name(&name) =>
        {
            let full = canonical_name_for_decl(namespace, &name);
            (!early.contains(&full.to_ascii_lowercase()))
                .then(|| Stmt::new(StmtKind::IncludeOnceMark { label: class_binding_mark_label(&full) }, span))
        }
        kind if is_discoverable_declaration(&kind) => None,
        StmtKind::NamespaceDecl { name } => {
            *current_namespace = Some(namespace_string(&name));
            Some(Stmt::new(StmtKind::NamespaceDecl { name }, span))
        }
        StmtKind::NamespaceBlock { name, body } => Some(Stmt::new(
            StmtKind::NamespaceBlock {
                body: strip_stmts(
                    body,
                    canonical,
                    function_variants,
                    Some(namespace_string(&name)),
                    early,
                ),
                name,
            },
            span,
        )),
        StmtKind::Synthetic(body) => {
            let body = strip_stmts(
                body,
                canonical,
                function_variants,
                current_namespace.clone(),
                early,
            );
            if body.is_empty() {
                None
            } else {
                Some(Stmt::new(StmtKind::Synthetic(body), span))
            }
        }
        other => Some(Stmt::new(other, span)),
    }
}

/// Returns true if the statement kind is a discoverable declaration that
/// should be extracted/stripped during include processing.
fn is_discoverable_declaration(kind: &StmtKind) -> bool {
    matches!(
        kind,
        StmtKind::FunctionDecl { .. }
            | StmtKind::ClassDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. }
            | StmtKind::PackedClassDecl { .. }
            | StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. }
    )
}
