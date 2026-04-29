use crate::parser::ast::{Stmt, StmtKind};
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

impl Checker {
    pub fn find_return_type_in_body(&mut self, body: &[Stmt], env: &TypeEnv) -> Option<PhpType> {
        let mut types = Vec::new();
        for stmt in body {
            self.collect_return_types(stmt, env, &mut types);
        }
        if types.is_empty() {
            return None;
        }
        let mut widest = types[0].clone();
        for ty in &types[1..] {
            widest = Self::wider_type(&widest, ty);
        }
        Some(widest)
    }

    pub fn find_return_type(&mut self, stmt: &Stmt, env: &TypeEnv) -> Option<PhpType> {
        let mut types = Vec::new();
        self.collect_return_types(stmt, env, &mut types);
        if types.is_empty() {
            return None;
        }
        let mut widest = types[0].clone();
        for ty in &types[1..] {
            widest = Self::wider_type(&widest, ty);
        }
        Some(widest)
    }

    pub(crate) fn collect_return_types(
        &mut self,
        stmt: &Stmt,
        env: &TypeEnv,
        types: &mut Vec<PhpType>,
    ) {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => {
                if let Ok(ty) = self.infer_type(expr, env) {
                    types.push(ty);
                }
            }
            StmtKind::Return(None) => {
                types.push(PhpType::Void);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                for s in then_body {
                    self.collect_return_types(s, env, types);
                }
                for (_, body) in elseif_clauses {
                    for s in body {
                        self.collect_return_types(s, env, types);
                    }
                }
                if let Some(body) = else_body {
                    for s in body {
                        self.collect_return_types(s, env, types);
                    }
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                for s in body {
                    self.collect_return_types(s, env, types);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                for s in try_body {
                    self.collect_return_types(s, env, types);
                }
                for catch_clause in catches {
                    for s in &catch_clause.body {
                        self.collect_return_types(s, env, types);
                    }
                }
                if let Some(body) = finally_body {
                    for s in body {
                        self.collect_return_types(s, env, types);
                    }
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    for s in body {
                        self.collect_return_types(s, env, types);
                    }
                }
                if let Some(body) = default {
                    for s in body {
                        self.collect_return_types(s, env, types);
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn body_contains_return(body: &[Stmt]) -> bool {
        body.iter().any(Self::stmt_contains_return)
    }

    fn stmt_contains_return(stmt: &Stmt) -> bool {
        match &stmt.kind {
            StmtKind::Return(_) => true,
            StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
                Self::body_contains_return(stmts)
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                Self::body_contains_return(then_body)
                    || elseif_clauses
                        .iter()
                        .any(|(_, body)| Self::body_contains_return(body))
                    || else_body
                        .as_ref()
                        .is_some_and(|body| Self::body_contains_return(body))
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Foreach { body, .. } => Self::body_contains_return(body),
            StmtKind::For {
                init, update, body, ..
            } => {
                init.as_ref()
                    .is_some_and(|stmt| Self::stmt_contains_return(stmt))
                    || update
                        .as_ref()
                        .is_some_and(|stmt| Self::stmt_contains_return(stmt))
                    || Self::body_contains_return(body)
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                Self::body_contains_return(try_body)
                    || catches
                        .iter()
                        .any(|catch_clause| Self::body_contains_return(&catch_clause.body))
                    || finally_body
                        .as_ref()
                        .is_some_and(|body| Self::body_contains_return(body))
            }
            StmtKind::Switch { cases, default, .. } => {
                cases
                    .iter()
                    .any(|(_, body)| Self::body_contains_return(body))
                    || default
                        .as_ref()
                        .is_some_and(|body| Self::body_contains_return(body))
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                Self::body_contains_return(then_body)
                    || else_body
                        .as_ref()
                        .is_some_and(|body| Self::body_contains_return(body))
            }
            _ => false,
        }
    }

    pub(crate) fn wider_type(a: &PhpType, b: &PhpType) -> PhpType {
        match (a, b) {
            _ if a == b => a.clone(),
            (PhpType::Str, _) | (_, PhpType::Str) => PhpType::Str,
            (PhpType::Float, _) | (_, PhpType::Float) => PhpType::Float,
            (PhpType::Void, other) | (other, PhpType::Void) => other.clone(),
            (PhpType::Never, other) | (other, PhpType::Never) => other.clone(),
            _ => PhpType::Mixed,
        }
    }
}
