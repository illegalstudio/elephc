//! Purpose:
//! Validates function returns semantics for the checker.
//! Keeps call diagnostics and return-flow analysis consistent with signatures and inferred expression types.
//!
//! Called from:
//! - `crate::types::checker::functions`
//!
//! Key details:
//! - Diagnostics should map shared planner errors back to source spans without duplicating call semantics.

use crate::errors::CompileError;
use crate::parser::ast::{Stmt, StmtKind};
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

#[derive(Clone)]
pub(crate) struct ReturnInfo {
    pub ty: PhpType,
    pub has_value: bool,
}

impl Checker {
    pub(crate) fn collect_return_infos(
        &mut self,
        stmt: &Stmt,
        env: &TypeEnv,
        returns: &mut Vec<ReturnInfo>,
    ) {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => {
                if let Ok(ty) = self.infer_type(expr, env) {
                    returns.push(ReturnInfo {
                        ty,
                        has_value: true,
                    });
                }
            }
            StmtKind::Return(None) => {
                returns.push(ReturnInfo {
                    ty: PhpType::Void,
                    has_value: false,
                });
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                for s in then_body {
                    self.collect_return_infos(s, env, returns);
                }
                for (_, body) in elseif_clauses {
                    for s in body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
                if let Some(body) = else_body {
                    for s in body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                for s in body {
                    self.collect_return_infos(s, env, returns);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                for s in try_body {
                    self.collect_return_infos(s, env, returns);
                }
                for catch_clause in catches {
                    for s in &catch_clause.body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
                if let Some(body) = finally_body {
                    for s in body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    for s in body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
                if let Some(body) = default {
                    for s in body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn body_contains_return(body: &[Stmt]) -> bool {
        body.iter().any(Self::stmt_contains_return)
    }

    pub(crate) fn require_declared_return_coverage(
        &self,
        declared_ret: &PhpType,
        body: &[Stmt],
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        if matches!(declared_ret, PhpType::Void | PhpType::Never) {
            return Ok(());
        }

        if crate::termination::block_guarantees_function_exit(body) {
            Ok(())
        } else {
            Err(CompileError::new(
                span,
                &format!("{} must return a value on every path", context),
            ))
        }
    }

    pub(crate) fn require_compatible_return_type(
        &self,
        expected: &PhpType,
        actual: &PhpType,
        has_value: bool,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        if !has_value {
            if matches!(expected, PhpType::Void) {
                return Ok(());
            }
            return Err(CompileError::new(
                span,
                &format!("{} must return a value of type {:?}", context, expected),
            ));
        }

        if matches!(expected, PhpType::Void) {
            return Err(CompileError::new(
                span,
                &format!("{} must not return a value", context),
            ));
        }

        if matches!(actual, PhpType::Void) && !Self::return_type_accepts_null(expected) {
            return Err(CompileError::new(
                span,
                &format!("{} expects {:?}, got Void", context, expected),
            ));
        }

        self.require_compatible_arg_type(expected, actual, span, context)
    }

    fn return_type_accepts_null(ty: &PhpType) -> bool {
        match ty {
            PhpType::Mixed => true,
            PhpType::Union(members) => members.iter().any(Self::return_type_accepts_null),
            PhpType::Void => true,
            _ => false,
        }
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
