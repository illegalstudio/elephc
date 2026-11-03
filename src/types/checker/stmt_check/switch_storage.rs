//! Purpose:
//! Preserves strict local assignment contracts across independently checked switch arms.
//!
//! Called from:
//! - Switch statement checking after each arm's source-ordered body check.
//!
//! Key details:
//! - Assignment compatibility is independent of the read views used at control-flow joins.
//! - Guard-only views do not count as stores, and an existing physical contract stays authoritative.

use std::collections::HashSet;

use crate::errors::CompileError;
use crate::parser::ast::{Expr, Stmt};
use crate::span::Span;
use crate::types::TypeEnv;

use super::super::loop_storage::{AssignedValue, LoopWrites, collect_value_assignment_stmt};
use super::super::Checker;

/// Seeds strict assignment replay from the bindings behind any temporary guard views.
pub(super) fn entry_bindings(checker: &Checker, env: &TypeEnv) -> TypeEnv {
    if !checker.strict_locals { return TypeEnv::new(); }
    env.iter().filter(|(name, _)| !name.starts_with('\u{1}')).map(|(name, view)| {
        let storage = checker.guarded_local_storage_type(&Expr::var(name), env).unwrap_or(view);
        (name.clone(), storage.clone())
    }).collect()
}

/// Merges actual arm stores without mistaking a narrowed read for an incompatible assignment.
pub(super) fn observe_arm(
    checker: &Checker, bindings: &mut TypeEnv, body: &[Stmt], exit: &TypeEnv, span: Span,
) -> Vec<CompileError> {
    if !checker.strict_locals { return Vec::new(); }
    let mut writes = LoopWrites::default();
    for stmt in body { collect_value_assignment_stmt(stmt, &mut writes); }
    let mut seen = HashSet::new();
    let mut errors = Vec::new();
    for (name, value) in writes.values {
        if !seen.insert(name) { continue; }
        let Some(ty) = exit.get(name) else { continue; };
        let merged = bindings.get(name).map_or_else(|| Some(ty.clone()), |previous|
            checker.merged_assignment_type(previous, ty));
        if let Some(merged) = merged {
            bindings.insert(name.to_string(), merged);
        } else {
            let span = match value { AssignedValue::Expr(expr) => expr.span, _ => span };
            errors.push(CompileError::new(span, &format!(
                "Type error: cannot reassign ${name} from {} to {ty}", bindings[name],
            )));
        }
    }
    errors
}
