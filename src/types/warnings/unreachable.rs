use crate::errors::CompileWarning;
use crate::parser::ast::{Stmt, StmtKind};
use crate::termination::stmt_guarantees_termination as shared_stmt_guarantees_termination;

pub(super) fn collect_unreachable_recursive(stmts: &[Stmt], warnings: &mut Vec<CompileWarning>) {
    collect_unreachable_in_block(stmts, warnings);
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_unreachable_recursive(then_body, warnings);
                for (_, body) in elseif_clauses {
                    collect_unreachable_recursive(body, warnings);
                }
                if let Some(body) = else_body {
                    collect_unreachable_recursive(body, warnings);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                collect_unreachable_recursive(then_body, warnings);
                if let Some(body) = else_body {
                    collect_unreachable_recursive(body, warnings);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Foreach { body, .. } => collect_unreachable_recursive(body, warnings),
            StmtKind::For {
                init, update, body, ..
            } => {
                if let Some(stmt) = init {
                    collect_unreachable_recursive(std::slice::from_ref(stmt), warnings);
                }
                if let Some(stmt) = update {
                    collect_unreachable_recursive(std::slice::from_ref(stmt), warnings);
                }
                collect_unreachable_recursive(body, warnings);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_unreachable_recursive(body, warnings);
                }
                if let Some(body) = default {
                    collect_unreachable_recursive(body, warnings);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_unreachable_recursive(try_body, warnings);
                for catch_clause in catches {
                    collect_unreachable_recursive(&catch_clause.body, warnings);
                }
                if let Some(body) = finally_body {
                    collect_unreachable_recursive(body, warnings);
                }
            }
            StmtKind::FunctionDecl { body, .. } => collect_unreachable_recursive(body, warnings),
            StmtKind::ClassDecl { methods, .. }
            | StmtKind::TraitDecl { methods, .. }
            | StmtKind::InterfaceDecl { methods, .. } => {
                for method in methods {
                    collect_unreachable_recursive(&method.body, warnings);
                }
            }
            StmtKind::NamespaceBlock { body, .. } => collect_unreachable_recursive(body, warnings),
            _ => {}
        }
    }
}

fn collect_unreachable_in_block(stmts: &[Stmt], warnings: &mut Vec<CompileWarning>) {
    let mut terminated = false;
    for stmt in stmts {
        if terminated {
            warnings.push(CompileWarning::new(stmt.span, "Unreachable code"));
            continue;
        }
        if stmt_guarantees_termination(stmt) {
            terminated = true;
        }
    }
}

fn stmt_guarantees_termination(stmt: &Stmt) -> bool {
    shared_stmt_guarantees_termination(stmt)
}
