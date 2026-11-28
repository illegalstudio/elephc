//! Purpose:
//! Retains operation-source profiles across prepared expression copies in AST visitors.
//!
//! Called from:
//! - Semantic source validation while walking statements and expression operands.
//!
//! Key details:
//! - A prepared producer overrides its physical consumer scope for its own expression tree.
//! - Nested statements start their own lexical profile, including closure bodies.
//! - Expression and statement exit hooks restore every enclosing policy.

use crate::{parser::ast::{Expr, Stmt}, source::{SourceMode, SourceProfile}};

/// Tracks physical statement profiles separately from prepared expression producers.
#[derive(Default)]
pub(crate) struct SourcePolicy {
    statements: Vec<SourceProfile>,
    expressions: Vec<(usize, SourceProfile)>,
}

impl SourcePolicy {
    /// Starts one physical statement without inheriting a surrounding closure expression's policy.
    pub(crate) fn enter_statement(&mut self, statement: &Stmt) {
        self.statements.push(SourceProfile {
            mode: statement.source_mode, strict_types: statement.strict_types,
        });
    }

    /// Restores the enclosing physical source after all nested statements have exited.
    pub(crate) fn leave_statement(&mut self) {
        self.statements.pop().expect("balanced statement source policy");
    }

    /// Selects producer policy only within the same statement's expression tree.
    pub(crate) fn enter_expression(&mut self, expression: &Expr) -> SourceProfile {
        let depth = self.statements.len();
        let profile = if expression.producer_binding.is_some() { expression.source_profile }
        else if let Some((_, profile)) = self.expressions.last().filter(|(owner, _)| *owner == depth) { *profile }
        else { self.statements.last().copied().unwrap_or_else(|| SourceProfile::new(SourceMode::Php)) };
        self.expressions.push((depth, profile));
        profile
    }

    /// Restores the parent expression's source without affecting nested statement scope.
    pub(crate) fn leave_expression(&mut self) {
        self.expressions.pop().expect("balanced expression source policy");
    }
}
