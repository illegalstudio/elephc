//! Purpose:
//! Identifies local declarations that cannot establish a path-independent integer write contract.
//!
//! Called from:
//! - `numeric_storage::lower_body` before speculative lowering and replay.
//!
//! Key details:
//! - Conditional declarations keep subsequent writes on their existing dynamic path.
//! - Linear declarations and explicit declaration initializers retain scalar integer sinks.
//! - Nested function-like bodies establish their own contracts and are not scanned here.

use std::collections::HashSet;

use crate::parser::ast::{Stmt, StmtKind};

/// Collects names whose declaration may be skipped, repeated or bypassed by an exception.
pub(super) fn conditional_declarations(body: &[Stmt]) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut pending: Vec<_> = body.iter().map(|stmt| (stmt, false)).collect();
    while let Some((stmt, conditional)) = pending.pop() {
        match &stmt.kind {
            StmtKind::TypedAssign { name, .. } => {
                if conditional { names.insert(name.clone()); }
            }
            StmtKind::Synthetic(body) | StmtKind::NamespaceBlock { body, .. } => {
                pending.extend(body.iter().map(|stmt| (stmt, conditional)));
            }
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                pending.extend(then_body.iter().map(|stmt| (stmt, true)));
                for (_, body) in elseif_clauses {
                    pending.extend(body.iter().map(|stmt| (stmt, true)));
                }
                if let Some(body) = else_body {
                    pending.extend(body.iter().map(|stmt| (stmt, true)));
                }
            }
            StmtKind::IfDef { then_body, else_body, .. } => {
                pending.extend(then_body.iter().map(|stmt| (stmt, true)));
                if let Some(body) = else_body {
                    pending.extend(body.iter().map(|stmt| (stmt, true)));
                }
            }
            StmtKind::While { body, .. } | StmtKind::DoWhile { body, .. }
            | StmtKind::Foreach { body, .. } | StmtKind::IncludeOnceGuard { body, .. } => {
                pending.extend(body.iter().map(|stmt| (stmt, true)));
            }
            StmtKind::For { init, update, body, .. } => {
                pending.extend(init.as_deref().map(|stmt| (stmt, conditional)));
                pending.extend(update.as_deref().map(|stmt| (stmt, true)));
                pending.extend(body.iter().map(|stmt| (stmt, true)));
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    pending.extend(body.iter().map(|stmt| (stmt, true)));
                }
                if let Some(body) = default {
                    pending.extend(body.iter().map(|stmt| (stmt, true)));
                }
            }
            StmtKind::Try { try_body, catches, finally_body } => {
                pending.extend(try_body.iter().map(|stmt| (stmt, true)));
                for catch in catches {
                    pending.extend(catch.body.iter().map(|stmt| (stmt, true)));
                }
                if let Some(body) = finally_body {
                    pending.extend(body.iter().map(|stmt| (stmt, true)));
                }
            }
            StmtKind::Echo(_) | StmtKind::Assign { .. } | StmtKind::RefAssign { .. }
            | StmtKind::ArrayAssign { .. } | StmtKind::NestedArrayAssign { .. }
            | StmtKind::ArrayPush { .. } | StmtKind::Include { .. }
            | StmtKind::IncludeOnceMark { .. } | StmtKind::Throw(_) | StmtKind::Break(_)
            | StmtKind::Continue(_) | StmtKind::ExprStmt(_) | StmtKind::NamespaceDecl { .. }
            | StmtKind::UseDecl { .. } | StmtKind::FunctionDecl { .. }
            | StmtKind::FunctionVariantGroup { .. } | StmtKind::FunctionVariantMark { .. }
            | StmtKind::Return(_) | StmtKind::ConstDecl { .. } | StmtKind::ListUnpack { .. }
            | StmtKind::Global { .. } | StmtKind::StaticVar { .. } | StmtKind::ClassDecl { .. }
            | StmtKind::EnumDecl { .. } | StmtKind::PackedClassDecl { .. }
            | StmtKind::InterfaceDecl { .. } | StmtKind::TraitDecl { .. }
            | StmtKind::PropertyAssign { .. } | StmtKind::StaticPropertyAssign { .. }
            | StmtKind::StaticPropertyArrayPush { .. } | StmtKind::StaticPropertyArrayAssign { .. }
            | StmtKind::PropertyArrayPush { .. } | StmtKind::PropertyArrayAssign { .. }
            | StmtKind::ExternFunctionDecl { .. } | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. } => {}
        }
    }
    names
}
