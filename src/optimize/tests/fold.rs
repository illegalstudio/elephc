//! Purpose:
//! Regression tests for optimizer fold behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

/// Verifies fold_constants evaluates (2+3)*4 to 20, respecting AST structure.
#[test]
fn test_fold_nested_integer_arithmetic() {
    let program = vec![Stmt::new(
        StmtKind::Echo(Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(Expr::int_lit(2)),
                        op: BinOp::Add,
                        right: Box::new(Expr::int_lit(3)),
                    },
                    Span::dummy(),
                )),
                op: BinOp::Mul,
                right: Box::new(Expr::int_lit(4)),
            },
            Span::dummy(),
        )),
        Span::dummy(),
    )];

    let folded = fold_constants(program);

    assert_eq!(folded, vec![Stmt::echo(Expr::int_lit(20))]);
}

/// Verifies 2 ** 3 is folded to FloatLiteral(8.0) — exponentiation yields float.
#[test]
fn test_fold_constant_pow_to_float_literal() {
    let program = vec![Stmt::echo(Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::int_lit(2)),
            op: BinOp::Pow,
            right: Box::new(Expr::int_lit(3)),
        },
        Span::dummy(),
    ))];

    let folded = fold_constants(program);

    assert_eq!(
        folded,
        vec![Stmt::echo(Expr::new(
            ExprKind::FloatLiteral(8.0),
            Span::dummy(),
        ))]
    );
}

/// Verifies division by zero is NOT folded — PHP would fatal, optimizer preserves the AST.
#[test]
fn test_skip_division_by_zero_fold() {
    let expr = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::int_lit(5)),
            op: BinOp::Div,
            right: Box::new(Expr::int_lit(0)),
        },
        Span::dummy(),
    );

    let folded = fold_constants(vec![Stmt::echo(expr.clone())]);

    assert_eq!(folded, vec![Stmt::echo(expr)]);
}

/// Verifies string concatenation is folded in class property defaults ("hello " + "world" -> "hello world").
#[test]
fn test_fold_string_concat_and_property_default() {
    let property = ClassProperty {
        name: "label".to_string(),
        visibility: Visibility::Public,
        type_expr: None,
        hooks: crate::parser::ast::PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        default: Some(Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::string_lit("hello ")),
                op: BinOp::Concat,
                right: Box::new(Expr::string_lit("world")),
            },
            Span::dummy(),
        )),
        span: Span::dummy(),
        attributes: Vec::new(),
    };

    let folded = fold_constants(vec![Stmt::new(
        StmtKind::ClassDecl {
            name: "Greeter".to_string(),
            extends: None,
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            trait_uses: Vec::new(),
            properties: vec![property],
            methods: Vec::new(),
        constants: Vec::new(),
        },
        Span::dummy(),
    )]);

    let StmtKind::ClassDecl { properties, .. } = &folded[0].kind else {
        panic!("expected class declaration");
    };
    assert_eq!(
        properties[0].default,
        Some(Expr::string_lit("hello world"))
    );
}

/// Verifies StrictEq (2===2), Lt (2.5<3.0), and Spaceship (2<=>3) are folded to true, true, -1.
#[test]
fn test_fold_strict_and_numeric_comparisons() {
    let program = vec![
        Stmt::echo(Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::int_lit(2)),
                op: BinOp::StrictEq,
                right: Box::new(Expr::int_lit(2)),
            },
            Span::dummy(),
        )),
        Stmt::echo(Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::float_lit(2.5)),
                op: BinOp::Lt,
                right: Box::new(Expr::float_lit(3.0)),
            },
            Span::dummy(),
        )),
        Stmt::echo(Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::int_lit(2)),
                op: BinOp::Spaceship,
                right: Box::new(Expr::int_lit(3)),
            },
            Span::dummy(),
        )),
    ];

    let folded = fold_constants(program);

    assert_eq!(
        folded,
        vec![
            Stmt::echo(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
            Stmt::echo(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
            Stmt::echo(Expr::int_lit(-1)),
        ]
    );
}

/// Verifies NullCoalesce (null ?? "fallback") and Ternary ("0" ? 10 : 20) fold to "fallback" and 20.
#[test]
fn test_fold_null_coalesce_and_ternary_only_for_scalar_constants() {
    let program = vec![
        Stmt::echo(Expr::new(
            ExprKind::NullCoalesce {
                value: Box::new(Expr::new(ExprKind::Null, Span::dummy())),
                default: Box::new(Expr::string_lit("fallback")),
            },
            Span::dummy(),
        )),
        Stmt::echo(Expr::new(
            ExprKind::Ternary {
                condition: Box::new(Expr::string_lit("0")),
                then_expr: Box::new(Expr::int_lit(10)),
                else_expr: Box::new(Expr::int_lit(20)),
            },
            Span::dummy(),
        )),
    ];

    let folded = fold_constants(program);

    assert_eq!(
        folded,
        vec![
            Stmt::echo(Expr::string_lit("fallback")),
            Stmt::echo(Expr::int_lit(20)),
        ]
    );
}

/// Verifies Or ("0" or "hello") folds to true and Not(!) on "0" folds to true using PHP truthiness.
#[test]
fn test_fold_logical_ops_and_not_using_php_truthiness() {
    let program = vec![
        Stmt::echo(Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::string_lit("0")),
                op: BinOp::Or,
                right: Box::new(Expr::string_lit("hello")),
            },
            Span::dummy(),
        )),
        Stmt::echo(Expr::new(
            ExprKind::Not(Box::new(Expr::string_lit("0"))),
            Span::dummy(),
        )),
    ];

    let folded = fold_constants(program);

    assert_eq!(
        folded,
        vec![
            Stmt::echo(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
            Stmt::echo(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
        ]
    );
}

/// Verifies int(float), float(string), bool(string), string(int) casts fold when result is unambiguous.
#[test]
fn test_fold_scalar_casts_when_result_is_unambiguous() {
    let program = vec![
        Stmt::echo(Expr::new(
            ExprKind::Cast {
                target: CastType::Int,
                expr: Box::new(Expr::float_lit(3.7)),
            },
            Span::dummy(),
        )),
        Stmt::echo(Expr::new(
            ExprKind::Cast {
                target: CastType::Float,
                expr: Box::new(Expr::string_lit("3.14")),
            },
            Span::dummy(),
        )),
        Stmt::echo(Expr::new(
            ExprKind::Cast {
                target: CastType::Bool,
                expr: Box::new(Expr::string_lit("0")),
            },
            Span::dummy(),
        )),
        Stmt::echo(Expr::new(
            ExprKind::Cast {
                target: CastType::String,
                expr: Box::new(Expr::int_lit(42)),
            },
            Span::dummy(),
        )),
    ];

    let folded = fold_constants(program);

    assert_eq!(
        folded,
        vec![
            Stmt::echo(Expr::int_lit(3)),
            Stmt::echo(Expr::float_lit(3.14)),
            Stmt::echo(Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
            Stmt::echo(Expr::string_lit("42")),
        ]
    );
}

/// Verifies int("42abc") is NOT folded — ambiguous string casts must stay unfolded.
#[test]
fn test_keep_ambiguous_string_casts_unfolded() {
    let expr = Expr::new(
        ExprKind::Cast {
            target: CastType::Int,
            expr: Box::new(Expr::string_lit("42abc")),
        },
        Span::dummy(),
    );

    let folded = fold_constants(vec![Stmt::echo(expr.clone())]);

    assert_eq!(folded, vec![Stmt::echo(expr)]);
}

/// Verifies `$items[0] = 5` result_target is dropped when structurally equal to target.
#[test]
fn test_fold_drops_assignment_result_target_when_equal_to_target() {
    // `$items[0] = 5` parses with `result_target = Some(target.clone())`
    // because the LHS is a non-local lvalue. Both fields end up structurally
    // identical after folding, so the optimizer drops the duplicate.
    let target = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::var("items")),
            index: Box::new(Expr::int_lit(0)),
        },
        Span::dummy(),
    );
    let assignment = Expr::new(
        ExprKind::Assignment {
            target: Box::new(target.clone()),
            value: Box::new(Expr::int_lit(5)),
            result_target: Some(Box::new(target.clone())),
            prelude: Vec::new(),
            conditional_value_temp: None,
        },
        Span::dummy(),
    );

    let folded = fold_constants(vec![Stmt::echo(assignment)]);

    let folded_kind = match &folded[0].kind {
        StmtKind::Echo(expr) => &expr.kind,
        other => panic!("expected Echo, got {:?}", other),
    };
    match folded_kind {
        ExprKind::Assignment { result_target, .. } => {
            assert!(result_target.is_none(), "expected result_target to be elided");
        }
        other => panic!("expected Assignment, got {:?}", other),
    }
}
