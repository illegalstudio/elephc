//! Purpose:
//! Verifies PHP arithmetic value types in shared native and reflection default metadata.
//!
//! Called from:
//! - The const-default resolver unit tests.
//!
//! Key details:
//! - Exact integer division and fitting powers stay integers on every supported target.
//! - Fractional, floating and overflow cases preserve PHP's float representation.

use super::*;
use crate::codegen::platform::Target;
use crate::span::Span;

/// Resolves one numeric default under every target without changing its PHP value type.
fn assert_arithmetic(op: BinOp, left: ExprKind, right: ExprKind, expected: Option<ConstDefaultValue>) {
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = Module::new(Target::parse(target).unwrap());
        let context = ConstDefaultContext { module: &module, current_class: None };
        let expression = Expr::new(ExprKind::BinaryOp {
            left: Box::new(Expr::new(left.clone(), Span::dummy())), op: op.clone(),
            right: Box::new(Expr::new(right.clone(), Span::dummy())),
        }, Span::dummy());
        assert!(resolve_const_default(&expression, &context) == expected, "{target}: {op:?}");
    }
}

/// Integer division and powers use exact checked integer arithmetic before float fallback.
#[test]
fn numeric_defaults_preserve_integer_results_on_all_targets() {
    for (op, left, right, expected) in [
        (BinOp::Div, 6, 2, 3), (BinOp::Div, -6, 2, -3),
        (BinOp::Pow, 6, 2, 36), (BinOp::Pow, -2, 3, -8),
        (BinOp::Pow, 3, 34, 16_677_181_699_666_569), (BinOp::Pow, 0, 0, 1),
    ] {
        assert_arithmetic(op, ExprKind::IntLiteral(left), ExprKind::IntLiteral(right),
            Some(const_default_int_value(expected)));
    }
}

/// Fractions, negative powers, overflow and explicit floats keep float results; zero division defers.
#[test]
fn numeric_defaults_preserve_float_and_error_boundaries_on_all_targets() {
    for (op, left, right, expected) in [
        (BinOp::Div, ExprKind::IntLiteral(6), ExprKind::IntLiteral(4), 1.5),
        (BinOp::Pow, ExprKind::IntLiteral(2), ExprKind::IntLiteral(-2), 0.25),
        (BinOp::Div, ExprKind::FloatLiteral(6.0), ExprKind::IntLiteral(2), 3.0),
        (BinOp::Pow, ExprKind::FloatLiteral(2.0), ExprKind::IntLiteral(2), 4.0),
        (BinOp::Div, ExprKind::IntLiteral(i64::MIN), ExprKind::IntLiteral(-1), -(i64::MIN as f64)),
        (BinOp::Add, ExprKind::IntLiteral(i64::MAX), ExprKind::IntLiteral(1), -(i64::MIN as f64)),
    ] {
        assert_arithmetic(op, left, right, Some(const_default_float(expected)));
    }
    assert_arithmetic(BinOp::Div, ExprKind::IntLiteral(1), ExprKind::IntLiteral(0), None);
}

/// PHP's MIN_INT remainder by negative one is zero, including after exact integer division.
#[test]
fn numeric_defaults_remainder_overflow_on_all_targets() {
    let divided = ExprKind::BinaryOp {
        left: Box::new(Expr::new(ExprKind::IntLiteral(i64::MIN), Span::dummy())),
        op: BinOp::Div,
        right: Box::new(Expr::new(ExprKind::IntLiteral(1), Span::dummy())),
    };
    for left in [ExprKind::IntLiteral(i64::MIN), divided] {
        assert_arithmetic(BinOp::Mod, left, ExprKind::IntLiteral(-1), Some(const_default_int_value(0)));
    }
    assert_arithmetic(BinOp::Mod, ExprKind::IntLiteral(1), ExprKind::IntLiteral(0), None);
}
