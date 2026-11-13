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
use crate::source::SourceMode;

thread_local! {
    static PROFILE: Cell<SemanticProfile> = const { Cell::new(SemanticProfile::PERMISSIVE) };
    static SOURCE_MODE: Cell<SourceMode> = const { Cell::new(SourceMode::Php) };
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

/// Restores the enclosing statement's source policy independently of the module profile.
pub(super) struct SourceModeGuard(SourceMode);

impl Drop for SourceModeGuard {
    /// Restores trusted/user nesting even when a recursive pass unwinds.
    fn drop(&mut self) { SOURCE_MODE.with(|slot| slot.set(self.0)); }
}

/// Selects implementation semantics for one statement without erasing nested user policies.
pub(super) fn scoped_source_mode(mode: SourceMode) -> SourceModeGuard {
    SourceModeGuard(SOURCE_MODE.with(|slot| slot.replace(mode)))
}

/// Projects the already resolved identity into legacy builtin optimizer inputs.
pub(super) fn active_profile() -> SemanticProfile { PROFILE.with(Cell::get) }

/// Projects the current statement policy without replacing its full module identity.
pub(super) fn active_source_mode() -> SourceMode { SOURCE_MODE.with(Cell::get) }

/// Tests one policy while keeping the immutable profile owned by the caller.
pub(super) fn enabled(feature: StrictFeature) -> bool {
    SOURCE_MODE.with(|slot| slot.get() != SourceMode::Internal)
        && PROFILE.with(|slot| slot.get().contains(feature))
}

/// Preserves catchable overflow unless literal operands prove an operation cannot fail.
pub(super) fn integer_overflow_may_throw(expr: &crate::parser::ast::Expr) -> bool {
    use crate::parser::ast::{BinOp, ExprKind};
    use elephc_semantics::numeric::{checked_integer, IntegerOp, NumericFailure};
    if !enabled(StrictFeature::Overflow) { return false; }
    match &expr.kind {
        ExprKind::Negate(inner) => match inner.kind {
            ExprKind::FloatLiteral(_) => false,
            ExprKind::IntLiteral(value) => value.checked_neg().is_none(),
            _ => true,
        },
        ExprKind::BinaryOp { left, op: op @ (BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Pow), right } => {
            if matches!(left.kind, ExprKind::FloatLiteral(_)) || matches!(right.kind, ExprKind::FloatLiteral(_)) { return false; }
            let (ExprKind::IntLiteral(left), ExprKind::IntLiteral(right)) = (&left.kind, &right.kind) else { return true; };
            let operation = match op { BinOp::Add => IntegerOp::Add, BinOp::Sub => IntegerOp::Sub,
                BinOp::Mul => IntegerOp::Mul, BinOp::Pow => IntegerOp::Pow, _ => unreachable!() };
            checked_integer(operation, *left, *right) == Err(NumericFailure::Overflow)
        }
        ExprKind::PreIncrement(_) | ExprKind::PostIncrement(_) | ExprKind::PreDecrement(_) | ExprKind::PostDecrement(_) => true,
        _ => false,
    }
}

/// Checked powers may reject numeric operands or invoke a warning handler during conversion.
pub(super) fn checked_power_conversion_may_dispatch(expr: &crate::parser::ast::Expr) -> bool {
    use crate::parser::ast::{BinOp, ExprKind};
    if !enabled(StrictFeature::Overflow) { return false; }
    let ExprKind::BinaryOp { left, op: BinOp::Pow, right } = &expr.kind else { return false; };
    [left, right].into_iter().any(|operand| match &operand.kind {
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::BoolLiteral(_) | ExprKind::Null => false,
        ExprKind::StringLiteral(value) => !crate::numeric_string::scan_numeric_prefix(value)
            .is_some_and(|scan| scan.is_fully_numeric()),
        _ => true,
    })
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

    /// User overflow stays executable while trusted implementation literals retain PHP promotion.
    #[test]
    fn strict_overflow_ast_folding_preserves_failures_and_source_modes() {
        use crate::parser::ast::{ExprKind, Stmt};
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
        let source = "<?php echo 9223372036854775807 + 1; echo -9223372036854775808 - 1; echo 9223372036854775807 * 2; echo 2 ** 63; echo -(-9223372036854775808);";
        let tokens = crate::lexer::tokenize_with_semantics(source, SourceMode::Php, 0, profile).unwrap();
        let parsed = crate::parser::parse(&tokens).unwrap();
        let _profile = scoped_semantic_profile(profile);
        for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
            let mut program = parsed.clone();
            for stmt in &mut program { stmt.source_mode = mode; }
            let folded = crate::optimize::fold_constants(program);
            for stmt in folded {
                let StmtKind::Echo(expr) = stmt.kind else { panic!("expected an echo"); };
                assert_eq!(matches!(expr.kind, ExprKind::FloatLiteral(_)), mode == SourceMode::Internal, "{mode:?} {expr:?}");
            }
        }
        let mut internal = parsed[0].clone();
        internal.source_mode = SourceMode::Internal;
        let mut wrapper = Stmt::new(StmtKind::Synthetic(vec![internal, parsed[0].clone()]), parsed[0].span);
        wrapper.source_mode = SourceMode::Internal;
        let folded = crate::optimize::fold_constants(vec![wrapper]);
        let StmtKind::Synthetic(children) = &folded[0].kind else { panic!("expected a wrapper"); };
        assert!(matches!(&children[0].kind, StmtKind::Echo(expr) if matches!(expr.kind, ExprKind::FloatLiteral(_))));
        assert!(matches!(&children[1].kind, StmtKind::Echo(expr) if matches!(expr.kind, ExprKind::BinaryOp { .. })));
        assert!(enabled(StrictFeature::Overflow));
    }
}
