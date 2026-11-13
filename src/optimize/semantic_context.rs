//! Purpose:
//! Adapts the explicit semantic profile to existing recursive AST optimizer helpers.
//!
//! Called from:
//! - Compiler orchestration and profile-aware optimizer entry points.
//!
//! Key details:
//! - The scoped adapter restores nested callers on drop and never selects backend semantics.

use std::cell::Cell;
use elephc_semantics::{SemanticProfile, StrictFeature};

thread_local! {
    static PROFILE: Cell<SemanticProfile> = const { Cell::new(SemanticProfile::PERMISSIVE) };
}

/// Restores the calling optimizer profile when a compilation or nested pass finishes.
pub struct SemanticProfileGuard(SemanticProfile);

impl Drop for SemanticProfileGuard {
    /// Restores on normal return and unwinding, including nested compilation.
    fn drop(&mut self) { PROFILE.with(|slot| slot.set(self.0)); }
}

/// Installs an already resolved profile for legacy recursive AST helper signatures.
pub fn scoped_semantic_profile(profile: SemanticProfile) -> SemanticProfileGuard {
    SemanticProfileGuard(PROFILE.with(|slot| slot.replace(profile)))
}

/// Tests one policy while keeping the immutable profile owned by the caller.
pub(super) fn enabled(feature: StrictFeature) -> bool {
    PROFILE.with(|slot| slot.get().contains(feature))
}

/// Conservatively preserves a condition guard unless syntax guarantees a boolean result.
pub(super) fn boolean_guard_may_throw(expr: &crate::parser::ast::Expr) -> bool {
    use crate::parser::ast::{BinOp, CastType, ExprKind};
    enabled(StrictFeature::Bools) && !matches!(&expr.kind,
        ExprKind::BoolLiteral(_) | ExprKind::Not(_)
        | ExprKind::Cast { target: CastType::Bool, .. }
        | ExprKind::InstanceOf { .. }
        | ExprKind::BinaryOp { op: BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Eq
            | BinOp::NotEq | BinOp::StrictEq | BinOp::StrictNotEq | BinOp::Lt
            | BinOp::LtEq | BinOp::Gt | BinOp::GtEq, .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;

    /// A folded builtin cannot erase a failing boolean operation before the checker runs.
    #[test]
    fn strict_profile_bools_folding_preserves_invalid_boolean_operands() {
        let _profile = scoped_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true));
        let tokens = crate::lexer::tokenize("<?php echo !strlen('x'); echo strlen('x') ? 1 : 2; echo strlen('x') && true;").unwrap();
        let ast = crate::parser::parse_with_mode(&tokens, crate::source::SourceMode::Php).unwrap();
        let folded = crate::optimize::fold_constants(ast);
        assert!(matches!(&folded.last().unwrap().kind, StmtKind::Echo(expr)
            if matches!(expr.kind, crate::parser::ast::ExprKind::BinaryOp { .. })));
        assert!(enabled(StrictFeature::Bools));
        {
            let _nested = scoped_semantic_profile(SemanticProfile::PERMISSIVE);
            assert!(!enabled(StrictFeature::Bools));
        }
        assert!(enabled(StrictFeature::Bools));
    }
}
