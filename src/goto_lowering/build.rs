//! Purpose:
//! Builds the AST nodes `goto` lowering synthesizes: the state-local assignments and tests, the
//! dispatch loop and its segment guards, and the `break`/`continue` exits.
//!
//! Called from:
//! - `crate::goto_lowering::rewrite`.
//!
//! Key details:
//! - Every synthesized node gets its own `Span::synthetic()`. Spans are keys, not only
//!   coordinates: loop storage contracts and local binding decisions are looked up by span, so
//!   two generated loops or two generated assignments must never share one.
//! - The lowering runs after parsing, when the parser's thread-local source profile is no longer
//!   installed, so each node copies the profile of the statement it was derived from.

use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind};
use crate::source::SourceMode;
use crate::span::Span;

/// The source profile a synthesized statement inherits from the statement it replaces.
#[derive(Debug, Clone, Copy)]
pub(super) struct Origin {
    source_mode: SourceMode,
    strict_types: bool,
}

impl Origin {
    /// Captures the source profile of `stmt`.
    pub(super) fn of(stmt: &Stmt) -> Self {
        Self {
            source_mode: stmt.source_mode,
            strict_types: stmt.strict_types,
        }
    }

    /// Builds a statement of `kind` at `span` under this profile.
    pub(super) fn stmt_at(self, kind: StmtKind, span: Span) -> Stmt {
        Stmt {
            kind,
            span,
            source_mode: self.source_mode,
            strict_types: self.strict_types,
            attributes: Vec::new(),
        }
    }

    /// Builds a statement of `kind` with a span of its own.
    pub(super) fn stmt(self, kind: StmtKind) -> Stmt {
        self.stmt_at(kind, Span::synthetic())
    }

    /// Builds `$state = value;`.
    pub(super) fn assign_state(self, state: &str, value: i64) -> Stmt {
        self.stmt(StmtKind::Assign {
            name: state.to_string(),
            value: int(value),
        })
    }

    /// Builds `while (true) { body }`.
    pub(super) fn dispatch_loop(self, body: Vec<Stmt>) -> Stmt {
        self.stmt(StmtKind::While {
            condition: expr(ExprKind::BoolLiteral(true)),
            body,
        })
    }

    /// Builds `if ($state === 0 || $state === a || …) { body }`: one segment of a dispatch
    /// loop, run when no jump is pending or when a pending jump resumes at one of `ids`.
    pub(super) fn guarded_segment(self, state: &str, ids: &[i64], body: Vec<Stmt>) -> Stmt {
        let condition = ids
            .iter()
            .map(|&id| state_is(state, id))
            .fold(state_is(state, 0), |left, right| binary(left, BinOp::Or, right));
        self.stmt(StmtKind::If {
            condition,
            then_body: body,
            elseif_clauses: Vec::new(),
            else_body: None,
        })
    }
}

/// Builds an expression node with a span of its own.
fn expr(kind: ExprKind) -> Expr {
    Expr::new(kind, Span::synthetic())
}

/// Builds an integer literal.
fn int(value: i64) -> Expr {
    expr(ExprKind::IntLiteral(value))
}

/// Builds a read of the local `name`.
fn variable(name: &str) -> Expr {
    expr(ExprKind::Variable(name.to_string()))
}

/// Builds `left <op> right`.
fn binary(left: Expr, op: BinOp, right: Expr) -> Expr {
    expr(ExprKind::BinaryOp {
        left: Box::new(left),
        op,
        right: Box::new(right),
    })
}

/// Builds `$state === value`.
fn state_is(state: &str, value: i64) -> Expr {
    binary(variable(state), BinOp::StrictEq, int(value))
}

/// Builds `$state === a || $state === b || …` for a non-empty list of label numbers.
fn state_is_any(state: &str, ids: &[i64]) -> Expr {
    ids.iter()
        .map(|&id| state_is(state, id))
        .reduce(|left, right| binary(left, BinOp::Or, right))
        .expect("a resumed branch routes at least one label")
}

/// Guards one `if`/`elseif` condition so a resumed jump takes the branch that holds its label.
///
/// `ids` are the labels inside this branch. `later_branch_resumes` says a branch after this one
/// holds a label too, in which case this condition must be false, and must not be evaluated,
/// while a jump is pending: `$state === 0 && condition`. A branch holding labels is taken
/// outright for them: `$state === a || …`.
pub(super) fn resume_guard(
    state: &str,
    ids: &[i64],
    later_branch_resumes: bool,
    condition: Expr,
) -> Expr {
    let natural = if later_branch_resumes {
        binary(state_is(state, 0), BinOp::And, condition)
    } else {
        condition
    };
    if ids.is_empty() {
        natural
    } else {
        binary(state_is_any(state, ids), BinOp::Or, natural)
    }
}
