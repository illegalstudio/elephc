//! Purpose:
//! Keeps original source branches while collecting only executable autoload rules.
//!
//! Called from:
//! - The semantic-aware autoload rule collector before type checking.
//!
//! Key details:
//! - Collection views never replace the source AST used for binding diagnostics.
//! - Inactive loader setup is stripped without registering its rules or removing live sources.

use std::collections::HashSet;
use crate::{parser::ast::{Program, StmtKind}, span::Span};

/// Collects every statically selectable branch and identifies leaves that cannot execute.
pub(super) fn alternatives(program: &Program) -> (Program, HashSet<Span>) {
    let mut out = Vec::new();
    let mut inactive = HashSet::new();
    append(program, false, &mut out, &mut inactive);
    (out, inactive)
}

/// Appends collection leaves while preserving inactivity inherited from an outer condition.
fn append(program: &Program, outer_inactive: bool, out: &mut Program, inactive: &mut HashSet<Span>) {
    for statement in program {
        if let StmtKind::If { condition, then_body, elseif_clauses, else_body } = &statement.kind {
            let selected = super::select_taken_branch(condition, elseif_clauses, else_body.is_some());
            if outer_inactive || !matches!(selected, super::BranchOutcome::Unfoldable) {
                for (index, body) in std::iter::once(then_body)
                    .chain(elseif_clauses.iter().map(|(_, body)| body)).chain(else_body.iter()).enumerate() {
                    let taken = matches!(selected, super::BranchOutcome::Taken(chosen) if chosen == index);
                    append(body, outer_inactive || !taken, out, inactive);
                }
                continue;
            }
        }
        if outer_inactive { inactive.insert(statement.span); }
        out.push(statement.clone());
    }
}

/// Removes consumed loader setup without erasing ordinary source statements or their control flow.
pub(super) fn restore(program: Program, consumed: &HashSet<Span>) -> Program {
    restore_in(program, consumed, false)
}

/// Retires unreachable loader setup even inside an otherwise runtime-dependent nested guard.
fn restore_in(program: Program, consumed: &HashSet<Span>, outer_inactive: bool) -> Program {
    program.into_iter().filter_map(|mut statement| {
        if consumed.contains(&statement.span) { return None; }
        if let StmtKind::If { condition, then_body, elseif_clauses, else_body } = &mut statement.kind {
            let selected = super::select_taken_branch(condition, elseif_clauses, else_body.is_some());
            if outer_inactive || !matches!(selected, super::BranchOutcome::Unfoldable) {
                for (index, body) in std::iter::once(then_body)
                    .chain(elseif_clauses.iter_mut().map(|(_, body)| body)).chain(else_body.iter_mut()).enumerate() {
                    let taken = matches!(selected, super::BranchOutcome::Taken(chosen) if chosen == index);
                    *body = restore_in(std::mem::take(body), consumed, outer_inactive || !taken);
                }
            }
        }
        Some(statement)
    }).collect()
}
