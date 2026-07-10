//! Purpose:
//! Regression tests for the `writes_globals` effect bit: callables that
//! declare `global` (directly or transitively) must be flagged, known builtins
//! must never be, and the pure-builtin list must stay free of by-ref params.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `writes_globals` feeds top-level propagation invalidation only; it must
//!   not change `is_observable`, so DCE decisions stay identical.

use super::*;

/// Builds a zero-parameter `FunctionDecl` statement with the given body.
fn function_decl(name: &str, body: Vec<Stmt>) -> Stmt {
    Stmt::new(
        StmtKind::FunctionDecl {
            name: name.to_string(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body,
        },
        Span::dummy(),
    )
}

/// A body declaring `global $g;` and assigning it is flagged `writes_globals`.
#[test]
fn test_global_declaring_function_writes_globals() {
    let program = vec![function_decl(
        "f",
        vec![
            Stmt::new(
                StmtKind::Global {
                    vars: vec!["g".to_string()],
                },
                Span::dummy(),
            ),
            Stmt::assign("g", Expr::int_lit(1)),
        ],
    )];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert!(
        function_effects.get("f").unwrap().writes_globals,
        "a body declaring `global` must be flagged writes_globals"
    );
}

/// The flag propagates transitively: a wrapper calling a global-writing
/// function is itself flagged.
#[test]
fn test_writes_globals_propagates_through_calls() {
    let program = vec![
        function_decl(
            "f",
            vec![
                Stmt::new(
                    StmtKind::Global {
                        vars: vec!["g".to_string()],
                    },
                    Span::dummy(),
                ),
                Stmt::assign("g", Expr::int_lit(1)),
            ],
        ),
        function_decl(
            "h",
            vec![Stmt::new(
                StmtKind::ExprStmt(Expr::new(
                    ExprKind::FunctionCall {
                        name: Name::from("f"),
                        args: Vec::new(),
                    },
                    Span::dummy(),
                )),
                Span::dummy(),
            )],
        ),
    ];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert!(
        function_effects.get("h").unwrap().writes_globals,
        "writes_globals must propagate through direct calls"
    );
}

/// A pure body — and a body calling only a known builtin — is not flagged.
#[test]
fn test_builtin_only_function_does_not_write_globals() {
    let program = vec![function_decl(
        "len",
        vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::from("strlen"),
                    args: vec![Expr::string_lit("abc")],
                },
                Span::dummy(),
            ))),
            Span::dummy(),
        )],
    )];

    let (function_effects, _, _) = compute_program_callable_effects(&program);

    assert!(
        !function_effects.get("len").unwrap().writes_globals,
        "known builtins never write PHP globals"
    );
}

/// A call to an unknown symbol stays conservative: flagged.
#[test]
fn test_unknown_callee_conservatively_writes_globals() {
    let expr = Expr::new(
        ExprKind::FunctionCall {
            name: Name::from("totally_unknown_symbol"),
            args: Vec::new(),
        },
        Span::dummy(),
    );

    assert!(
        expr_effect(&expr).writes_globals,
        "unknown callees must stay conservative for globals"
    );
}

/// A known non-pure builtin (`sort`) writes its by-ref argument, not globals.
#[test]
fn test_known_builtin_call_does_not_write_globals() {
    let expr = Expr::new(
        ExprKind::FunctionCall {
            name: Name::from("sort"),
            args: vec![Expr::var("a")],
        },
        Span::dummy(),
    );

    assert!(
        !expr_effect(&expr).writes_globals,
        "registry builtins cannot write PHP globals"
    );
}

/// Every builtin on the pure-non-throwing list must be by-value only:
/// `propagate_args` substitution into pure calls relies on it.
#[test]
fn test_pure_builtin_list_has_no_by_ref_params() {
    for name in crate::builtins::registry::names() {
        if !is_pure_non_throwing_builtin(name) {
            continue;
        }
        let Some(def) = crate::builtins::registry::lookup(name) else {
            continue;
        };
        assert!(
            def.ref_params.iter().all(|by_ref| !by_ref),
            "pure-non-throwing builtin `{name}` must not take by-ref parameters"
        );
    }
}
