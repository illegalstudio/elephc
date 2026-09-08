//! Purpose:
//! Rewrites the AST to remove declarations outside the computed reachability sets.
//! Recurses through every grouping shape used by declaration discovery and EIR lowering.
//!
//! Called from:
//! - `crate::optimize::reachability::prune_unreachable_declarations()`.
//!
//! Key details:
//! - Non-declaration statements are never removed by this pass.
//! - Live class-like declarations retain only methods proven reachable by the graph.
//! - Trait AST bodies remain as flattening sources; consuming `CheckResult` methods are pruned.

use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Program, Stmt, StmtKind};

use super::graph::Reachability;

/// Prunes declarations throughout a complete statement tree.
pub(super) fn program(program: Program, reachability: &Reachability) -> Program {
    prune_statements(program, reachability)
}

/// Retains live declarations and recursively rewrites nested statement lists.
fn prune_statements(statements: Program, reachability: &Reachability) -> Program {
    statements
        .into_iter()
        .filter_map(|mut statement| {
            prune_statement_children(&mut statement, reachability);
            declaration_is_live(&statement, reachability).then_some(statement)
        })
        .collect()
}

/// Rewrites nested declaration-bearing statement lists and class method lists in place.
fn prune_statement_children(statement: &mut Stmt, reachability: &Reachability) {
    match &mut statement.kind {
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            *then_body = prune_statements(std::mem::take(then_body), reachability);
            for (_, body) in elseif_clauses {
                *body = prune_statements(std::mem::take(body), reachability);
            }
            if let Some(body) = else_body {
                *body = prune_statements(std::mem::take(body), reachability);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            *then_body = prune_statements(std::mem::take(then_body), reachability);
            if let Some(body) = else_body {
                *body = prune_statements(std::mem::take(body), reachability);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => {
            *body = prune_statements(std::mem::take(body), reachability);
        }
        StmtKind::For {
            init, update, body, ..
        } => {
            if let Some(initializer) = init {
                prune_statement_children(initializer, reachability);
            }
            if let Some(update) = update {
                prune_statement_children(update, reachability);
            }
            *body = prune_statements(std::mem::take(body), reachability);
        }
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                *body = prune_statements(std::mem::take(body), reachability);
            }
            if let Some(body) = default {
                *body = prune_statements(std::mem::take(body), reachability);
            }
        }
        StmtKind::Synthetic(body) => {
            *body = prune_statements(std::mem::take(body), reachability);
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            *try_body = prune_statements(std::mem::take(try_body), reachability);
            for catch in catches {
                catch.body = prune_statements(std::mem::take(&mut catch.body), reachability);
            }
            if let Some(body) = finally_body {
                *body = prune_statements(std::mem::take(body), reachability);
            }
        }
        StmtKind::ClassDecl { name, methods, .. }
        | StmtKind::EnumDecl { name, methods, .. }
        | StmtKind::InterfaceDecl { name, methods, .. } => {
            retain_methods(name, methods, reachability);
        }
        // Trait bodies are compile-time flattening sources, not independently emitted methods.
        // Keep their source declarations intact; `CheckResult.method_decls` on each consuming
        // class is the authoritative emitted surface and is pruned during reconciliation.
        StmtKind::TraitDecl { .. } => {}
        StmtKind::Echo(_)
        | StmtKind::Assign { .. }
        | StmtKind::RefAssign { .. }
        | StmtKind::ArrayAssign { .. }
        | StmtKind::NestedArrayAssign { .. }
        | StmtKind::ArrayPush { .. }
        | StmtKind::TypedAssign { .. }
        | StmtKind::Include { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::Throw(_)
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::ExprStmt(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::FunctionDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::Return(_)
        | StmtKind::ConstDecl { .. }
        | StmtKind::ListUnpack { .. }
        | StmtKind::Global { .. }
        | StmtKind::StaticVar { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::PropertyAssign { .. }
        | StmtKind::DynamicPropertyArrayPush { .. }
        | StmtKind::StaticPropertyAssign { .. }
        | StmtKind::StaticPropertyArrayPush { .. }
        | StmtKind::StaticPropertyArrayAssign { .. }
        | StmtKind::PropertyArrayPush { .. }
        | StmtKind::PropertyArrayAssign { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => {}
    }
}

/// Retains methods selected for one live class-like declaration.
fn retain_methods(class_name: &str, methods: &mut Vec<ClassMethod>, reachability: &Reachability) {
    let class_key = php_symbol_key(class_name);
    methods.retain(|method| {
        reachability.methods.contains(&(
            class_key.clone(),
            php_symbol_key(&method.name),
            method.is_static,
        ))
    });
}

/// Returns whether a statement-level declaration belongs to the final keep-set.
fn declaration_is_live(statement: &Stmt, reachability: &Reachability) -> bool {
    match &statement.kind {
        StmtKind::FunctionDecl { name, .. } => {
            reachability.functions.contains(&php_symbol_key(name))
        }
        StmtKind::FunctionVariantGroup { name, variants } => {
            reachability.functions.contains(&php_symbol_key(name))
                || variants
                    .iter()
                    .any(|variant| reachability.functions.contains(&php_symbol_key(variant)))
        }
        StmtKind::FunctionVariantMark { name, variant } => {
            reachability.functions.contains(&php_symbol_key(name))
                || reachability.functions.contains(&php_symbol_key(variant))
        }
        StmtKind::ClassDecl { name, .. }
        | StmtKind::EnumDecl { name, .. }
        | StmtKind::InterfaceDecl { name, .. }
        | StmtKind::TraitDecl { name, .. }
        | StmtKind::PackedClassDecl { name, .. }
        | StmtKind::ExternClassDecl { name, .. } => {
            reachability.classes.contains(&php_symbol_key(name))
        }
        StmtKind::ExternFunctionDecl { name, .. } => {
            reachability.externs.contains(&php_symbol_key(name))
        }
        _ => true,
    }
}
