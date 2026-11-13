//! Purpose:
//! Adapts shared strict guarantees to flow-sensitive checker types.
//!
//! Called from:
//! - Condition, operator, binding, and value-boundary checking.
//!
//! Key details:
//! - Known invalid kinds are diagnostics; dynamic values retain runtime guards.

use super::Checker;
use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};
use elephc_semantics::StrictFeature;

impl Checker {
    /// Checks the PHP-visible boolean contract without restricting internal implementation tests.
    pub(crate) fn require_boolean_type(&self, ty: &PhpType, span: Span) -> Result<(), CompileError> {
        if self.semantic_source_mode == crate::source::SourceMode::Internal
            || !self.semantic_profile.contains(StrictFeature::Bools) { return Ok(()); }
        if boolean_possible(ty) { return Ok(()); }
        Err(CompileError::new(span, &format!("--strict-bools requires bool, got {ty}")))
    }

    /// Infers side effects once before validating a conditional operand.
    pub(crate) fn infer_condition_type_with_effects(&mut self, expr: &Expr, env: &mut TypeEnv) -> Result<PhpType, CompileError> {
        let ty = self.infer_type_with_assignment_effects(expr, env)?;
        self.require_boolean_type(&ty, expr.span)?;
        Ok(ty)
    }
}

/// Returns whether a value can be boolean without an implicit conversion.
fn boolean_possible(ty: &PhpType) -> bool {
    match ty {
        PhpType::Bool | PhpType::False | PhpType::Mixed | PhpType::Never => true,
        PhpType::Union(members) => members.iter().any(boolean_possible),
        _ => false,
    }
}
