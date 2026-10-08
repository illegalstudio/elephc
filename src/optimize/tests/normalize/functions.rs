//! Purpose:
//! Regression tests for optimizer normalize behavior on function and method bodies over parser
//! AST fixtures: a bare trailing `return;` is dropped, including through trailing `if` and
//! `try` branches, while by-reference-returning functions and generators keep it.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

/// Builds a `function <name>() { body }` declaration.
fn function_decl(name: &str, by_ref_return: bool, body: Vec<Stmt>) -> Stmt {
    Stmt::new(
        StmtKind::FunctionDecl {
            name: name.into(),
            type_params: Vec::new(),
            params: Vec::new(),
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return,
            body,
        },
        Span::dummy(),
    )
}

/// Builds a bare `return;`.
fn bare_return() -> Stmt {
    Stmt::new(StmtKind::Return(None), Span::dummy())
}

/// `function f() { echo 1; return; }` becomes `function f() { echo 1; }`.
#[test]
fn test_normalize_control_flow_drops_trailing_bare_return_from_function_body() {
    let program = vec![function_decl(
        "f",
        false,
        vec![Stmt::echo(Expr::int_lit(1)), bare_return()],
    )];

    let normalized = normalize_control_flow(program);

    assert_eq!(
        normalized,
        vec![function_decl("f", false, vec![Stmt::echo(Expr::int_lit(1))])]
    );
}

/// `function f() { if ($a) { echo 1; return; } else { echo 2; return; } }` drops both trailing
/// returns and keeps the branch shell: `if ($a) { echo 1; } else { echo 2; }`.
#[test]
fn test_normalize_control_flow_drops_trailing_bare_returns_through_if_branches() {
    let program = vec![function_decl(
        "f",
        false,
        vec![Stmt::new(
            StmtKind::If {
                condition: Expr::var("a"),
                then_body: vec![Stmt::echo(Expr::int_lit(1)), bare_return()],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![Stmt::echo(Expr::int_lit(2)), bare_return()]),
            },
            Span::dummy(),
        )],
    )];

    let normalized = normalize_control_flow(program);

    assert_eq!(
        normalized,
        vec![function_decl(
            "f",
            false,
            vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("a"),
                    then_body: vec![Stmt::echo(Expr::int_lit(1))],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::echo(Expr::int_lit(2))]),
                },
                Span::dummy(),
            )],
        )]
    );
}

/// A `return;` that is not on the tail path stays: `if ($a) { return; } echo 2;` is an early
/// exit, and only the final `return;` after `echo 2` is dropped.
#[test]
fn test_normalize_control_flow_keeps_early_bare_return() {
    let early_exit = Stmt::new(
        StmtKind::If {
            condition: Expr::var("a"),
            then_body: vec![bare_return()],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        Span::dummy(),
    );
    let program = vec![function_decl(
        "f",
        false,
        vec![early_exit.clone(), Stmt::echo(Expr::int_lit(2)), bare_return()],
    )];

    let normalized = normalize_control_flow(program);

    assert_eq!(
        normalized,
        vec![function_decl(
            "f",
            false,
            vec![early_exit, Stmt::echo(Expr::int_lit(2))],
        )]
    );
}

/// `function f() { try { $q = $a / $b; return; } finally { echo 1; } }` drops the `return;`
/// inside the `try` body; the division may throw, so the `try` shell itself stays and the
/// `finally` still runs on the way out.
#[test]
fn test_normalize_control_flow_drops_trailing_bare_return_inside_try_body() {
    let risky_call = Stmt::assign(
        "q",
        Expr::binop(Expr::var("a"), BinOp::Div, Expr::var("b")),
    );
    let program = vec![function_decl(
        "f",
        false,
        vec![Stmt::new(
            StmtKind::Try {
                try_body: vec![risky_call.clone(), bare_return()],
                catches: Vec::new(),
                finally_body: Some(vec![Stmt::echo(Expr::int_lit(1))]),
            },
            Span::dummy(),
        )],
    )];

    let normalized = normalize_control_flow(program);

    assert_eq!(
        normalized,
        vec![function_decl(
            "f",
            false,
            vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![risky_call],
                    catches: Vec::new(),
                    finally_body: Some(vec![Stmt::echo(Expr::int_lit(1))]),
                },
                Span::dummy(),
            )],
        )]
    );
}

/// `function &f() { echo 1; return; }` keeps its trailing `return;`.
#[test]
fn test_normalize_control_flow_keeps_trailing_bare_return_in_by_ref_function() {
    let body = vec![Stmt::echo(Expr::int_lit(1)), bare_return()];
    let program = vec![function_decl("f", true, body.clone())];

    let normalized = normalize_control_flow(program);

    assert_eq!(normalized, vec![function_decl("f", true, body)]);
}

/// A generator body keeps its trailing `return;` so the generator pipeline sees the body the
/// checker validated.
#[test]
fn test_normalize_control_flow_keeps_trailing_bare_return_in_generator() {
    let yield_stmt = Stmt::new(
        StmtKind::ExprStmt(Expr::new(
            ExprKind::Yield {
                key: None,
                value: Some(Box::new(Expr::int_lit(1))),
            },
            Span::dummy(),
        )),
        Span::dummy(),
    );
    let body = vec![yield_stmt, bare_return()];
    let program = vec![function_decl("gen", false, body.clone())];

    let normalized = normalize_control_flow(program);

    assert_eq!(normalized, vec![function_decl("gen", false, body)]);
}

/// A valued `return 1;` is never a bare return and stays where it is.
#[test]
fn test_normalize_control_flow_keeps_trailing_valued_return() {
    let body = vec![Stmt::new(
        StmtKind::Return(Some(Expr::int_lit(1))),
        Span::dummy(),
    )];
    let program = vec![function_decl("f", false, body.clone())];

    let normalized = normalize_control_flow(program);

    assert_eq!(normalized, vec![function_decl("f", false, body)]);
}
