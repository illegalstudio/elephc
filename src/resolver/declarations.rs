//! Purpose:
//! Extracts and strips declarations that can be discovered from included files.
//! Separates declaration availability from runtime execution of include statements.
//!
//! Called from:
//! - `crate::resolver::discovery` and include resolution paths.
//!
//! Key details:
//! - Discoverable declarations must keep namespace context and include-loaded function variant metadata.

use std::path::Path;

use crate::names::canonical_name_for_decl;
use crate::parser::ast::{Stmt, StmtKind};

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
pub(super) fn strip_discoverable_declarations(
    stmts: Vec<Stmt>,
    canonical: Option<&Path>,
    function_variants: &FunctionVariantRegistry,
) -> Vec<Stmt> {
    strip_stmts(stmts, canonical, function_variants, None)
}

/// Internal recursive helper that processes statements and strips discoverable
/// declarations, tracking the current namespace context via `current_namespace`.
/// The `namespace` parameter carries the effective namespace for the current block.
fn strip_stmts(
    stmts: Vec<Stmt>,
    canonical: Option<&Path>,
    function_variants: &FunctionVariantRegistry,
    namespace: Option<String>,
) -> Vec<Stmt> {
    let mut stripped = Vec::new();
    let mut namespace = namespace;
    for stmt in stmts {
        let stmt_namespace = namespace.clone();
        if let Some(stmt) = strip_stmt(
            stmt,
            canonical,
            function_variants,
            stmt_namespace.as_deref(),
            &mut namespace,
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
