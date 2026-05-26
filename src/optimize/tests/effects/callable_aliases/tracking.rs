//! Purpose:
//! Regression tests for optimizer effects callable_aliases tracking behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

// Verifies that a closure stored in a local variable is tracked as a callable alias.
// The function `relay` captures a closure in `$f` and calls it; effects must reflect that
// the captured closure (which calls `strlen`) is reachable through `$f`.
#[test]
fn test_program_function_effects_track_closure_alias_locals() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "relay".to_string(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Assign {
                        name: "f".to_string(),
                        value: Expr::new(
                            ExprKind::Closure {
                                params: Vec::new(),
                                variadic: None,
                                return_type: None,
                                body: vec![Stmt::new(
                                    StmtKind::Return(Some(Expr::new(
                                        ExprKind::FunctionCall {
                                            name: Name::from("strlen"),
                                            args: vec![Expr::string_lit("abc")],
                                        },
                                        Span::dummy(),
                                    ))),
                                    Span::dummy(),
                                )],
                                is_arrow: false,
                                is_static: false,
                                captures: Vec::new(),
                                capture_refs: Vec::new(),
                            },
                            Span::dummy(),
                        ),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::Return(Some(Expr::new(
                        ExprKind::ClosureCall {
                            var: "f".to_string(),
                            args: Vec::new(),
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                ),
            ],
        },
        Span::dummy(),
    )];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert_eq!(
        function_effects.get("relay"),
        Some(&Effect::PURE.with_side_effects())
    );
}

// Verifies that a ternary expression producing a first-class callable preserves alias tracking.
// `$flag ? strlen(...) : strlen(...)` assigns the same callable target in both branches;
// calling `$f(...)` after assignment must reflect that `strlen` is the reachable target.
#[test]
fn test_program_function_effects_track_callable_alias_through_ternary() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "relay".to_string(),
            params: vec![("flag".to_string(), None, None, false)],
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Assign {
                        name: "f".to_string(),
                        value: Expr::new(
                            ExprKind::Ternary {
                                condition: Box::new(Expr::var("flag")),
                                then_expr: Box::new(Expr::new(
                                    ExprKind::FirstClassCallable(CallableTarget::Function(
                                        Name::from("strlen"),
                                    )),
                                    Span::dummy(),
                                )),
                                else_expr: Box::new(Expr::new(
                                    ExprKind::FirstClassCallable(CallableTarget::Function(
                                        Name::from("strlen"),
                                    )),
                                    Span::dummy(),
                                )),
                            },
                            Span::dummy(),
                        ),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::Return(Some(Expr::new(
                        ExprKind::ClosureCall {
                            var: "f".to_string(),
                            args: vec![Expr::string_lit("abc")],
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                ),
            ],
        },
        Span::dummy(),
    )];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert_eq!(
        function_effects.get("relay"),
        Some(&Effect::PURE.with_side_effects())
    );
}

// Verifies that a match expression producing a first-class callable preserves alias tracking.
// `match ($flag) { 1 => strlen(...), default => strlen(...) }` assigns the same callable
// target in all arms; calling `$f(...)` after assignment must reflect `strlen` reachability.
#[test]
fn test_program_function_effects_track_callable_alias_through_match() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "relay".to_string(),
            params: vec![("flag".to_string(), None, None, false)],
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Assign {
                        name: "f".to_string(),
                        value: Expr::new(
                            ExprKind::Match {
                                subject: Box::new(Expr::var("flag")),
                                arms: vec![(
                                    vec![Expr::int_lit(1)],
                                    Expr::new(
                                        ExprKind::FirstClassCallable(
                                            CallableTarget::Function(Name::from("strlen")),
                                        ),
                                        Span::dummy(),
                                    ),
                                )],
                                default: Some(Box::new(Expr::new(
                                    ExprKind::FirstClassCallable(CallableTarget::Function(
                                        Name::from("strlen"),
                                    )),
                                    Span::dummy(),
                                ))),
                            },
                            Span::dummy(),
                        ),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::Return(Some(Expr::new(
                        ExprKind::ClosureCall {
                            var: "f".to_string(),
                            args: vec![Expr::string_lit("abc")],
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                ),
            ],
        },
        Span::dummy(),
    )];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert_eq!(
        function_effects.get("relay"),
        Some(&Effect::PURE.with_side_effects())
    );
}

// Verifies that a null-coalesce expression producing a first-class callable preserves alias tracking.
// `strlen(...) ?? strlen(...)` assigns the same callable target in both operands;
// calling `$f(...)` after assignment must reflect that `strlen` is the reachable target.
#[test]
fn test_program_function_effects_track_callable_alias_through_null_coalesce() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "relay".to_string(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Assign {
                        name: "f".to_string(),
                        value: Expr::new(
                            ExprKind::NullCoalesce {
                                value: Box::new(Expr::new(
                                    ExprKind::FirstClassCallable(CallableTarget::Function(
                                        Name::from("strlen"),
                                    )),
                                    Span::dummy(),
                                )),
                                default: Box::new(Expr::new(
                                    ExprKind::FirstClassCallable(CallableTarget::Function(
                                        Name::from("strlen"),
                                    )),
                                    Span::dummy(),
                                )),
                            },
                            Span::dummy(),
                        ),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::Return(Some(Expr::new(
                        ExprKind::ClosureCall {
                            var: "f".to_string(),
                            args: vec![Expr::string_lit("abc")],
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                ),
            ],
        },
        Span::dummy(),
    )];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert_eq!(
        function_effects.get("relay"),
        Some(&Effect::PURE.with_side_effects())
    );
}

// Verifies that chained variable assignments propagate callable alias tracking.
// `$f = strlen(...); $g = $f;` followed by calling `$g(...)` must track `strlen` as the
// reachable callable, demonstrating that alias copy does not break tracking.
#[test]
fn test_program_function_effects_track_callable_alias_locals() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "relay".to_string(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Assign {
                        name: "f".to_string(),
                        value: Expr::new(
                            ExprKind::FirstClassCallable(CallableTarget::Function(Name::from(
                                "strlen",
                            ))),
                            Span::dummy(),
                        ),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::Assign {
                        name: "g".to_string(),
                        value: Expr::var("f"),
                    },
                    Span::dummy(),
                ),
                Stmt::new(
                    StmtKind::Return(Some(Expr::new(
                        ExprKind::ClosureCall {
                            var: "g".to_string(),
                            args: vec![Expr::string_lit("abc")],
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                ),
            ],
        },
        Span::dummy(),
    )];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert_eq!(
        function_effects.get("relay"),
        Some(&Effect::PURE.with_side_effects())
    );
}
