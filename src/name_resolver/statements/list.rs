//! Purpose:
//! Resolves statement lists while updating namespace and use-import context.
//! Flattens namespace blocks and routes declarations separately from ordinary statements.
//!
//! Called from:
//! - `crate::name_resolver::resolve()` and nested body resolvers.
//!
//! Key details:
//! - `use` statements affect following statements in the current namespace list, matching PHP ordering.

use crate::errors::CompileError;
use crate::parser::ast::{Stmt, StmtKind};

use super::context::ResolveContext;
use super::rewrite::resolve_regular_stmt;
use super::super::declarations::resolve_decl_stmt;
use super::super::names::register_imports;
use super::super::{namespace_name, Imports, Symbols};

/// Resolves a list of statements, applying namespace boundaries and use imports.
///
/// Iterates through statements in order, maintaining the current namespace and import
/// context. `NamespaceDecl` switches the active namespace and resets imports.
/// `NamespaceBlock` is flattened into the parent list via recursive resolution.
/// `UseDecl` registers imports that affect subsequent statements. All other statements
/// are routed through declaration resolution first, then regular statement resolution.
///
/// Returns the resolved statement list with rewritten names and imported symbols.
pub(in crate::name_resolver) fn resolve_stmt_list(
    stmts: &[Stmt],
    current_namespace: Option<&str>,
    incoming_imports: &Imports,
    symbols: &Symbols,
) -> Result<Vec<Stmt>, CompileError> {
    let mut resolved = Vec::new();
    let mut namespace = current_namespace.map(str::to_string);
    let mut imports = incoming_imports.clone();

    for stmt in stmts {
        match &stmt.kind {
            StmtKind::NamespaceDecl { name } => {
                namespace = Some(namespace_name(name));
                imports = Imports::default();
            }
            StmtKind::NamespaceBlock { name, body } => {
                let block_namespace = Some(namespace_name(name));
                let body =
                    resolve_stmt_list(body, block_namespace.as_deref(), &Imports::default(), symbols)?;
                resolved.extend(body);
            }
            StmtKind::UseDecl { imports: use_items } => {
                register_imports(&mut imports, use_items, stmt.span)?;
            }
            _ => {
                if let Some(resolved_stmt) =
                    resolve_decl_stmt(stmt, namespace.as_deref(), &imports, symbols)?
                {
                    resolved.push(resolved_stmt);
                    continue;
                }

                let ctx = ResolveContext::new(namespace.as_deref(), &imports, symbols);
                resolved.push(resolve_regular_stmt(stmt, ctx)?);
            }
        }
    }

    Ok(resolved)
}
