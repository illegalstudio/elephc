//! Purpose:
//! Regression tests for targeted invalidation wired into propagation: calls,
//! array reads, complex `unset`s, and loop bodies invalidate only the locals
//! they can write instead of clearing the whole constant environment.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Fixtures pair an "unrelated fact survives" assertion with a "touched
//!   fact dies" assertion so both precision and soundness are locked.

use super::*;

/// Builds a `FunctionCall` expression statement.
fn call_stmt(name: &str, args: Vec<Expr>) -> Stmt {
    Stmt::new(
        StmtKind::ExprStmt(Expr::new(
            ExprKind::FunctionCall {
                name: Name::from(name),
                args,
            },
            Span::dummy(),
        )),
        Span::dummy(),
    )
}

/// Builds a user function declaration that echoes a string (output side
/// effect, no local/global writes).
fn noisy_function(name: &str) -> Stmt {
    Stmt::new(
        StmtKind::FunctionDecl {
            name: name.to_string(),
            params: vec![("p".to_string(), None, None, false)],
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::echo(Expr::string_lit("hi"))],
        },
        Span::dummy(),
    )
}

/// A user function with output side effects but no by-ref params and no
/// `global` cannot write the caller's locals: facts survive the call.
#[test]
fn test_output_only_user_call_keeps_unrelated_facts() {
    let program = vec![
        noisy_function("noisy"),
        Stmt::assign("x", Expr::int_lit(5)),
        call_stmt("noisy", vec![Expr::var("y")]),
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[3],
        Stmt::echo(Expr::int_lit(6)),
        "an output-only callee cannot write caller locals"
    );
}

/// `sort($a)` invalidates `$a` but keeps unrelated scalar facts.
#[test]
fn test_by_ref_builtin_keeps_unrelated_facts() {
    let program = vec![
        Stmt::assign("x", Expr::int_lit(5)),
        call_stmt("sort", vec![Expr::var("a")]),
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::int_lit(6)),
        "sort($a) can only write $a"
    );
}

/// A by-ref builtin does invalidate the argument it mutates.
#[test]
fn test_by_ref_builtin_invalidates_its_argument() {
    let program = vec![
        Stmt::assign("n", Expr::int_lit(5)),
        // `settype($n, 'float')` mutates `$n` by reference — but any by-ref
        // builtin works here; `sort` keeps the fixture registry-stable.
        call_stmt("sort", vec![Expr::var("n")]),
        Stmt::echo(Expr::binop(Expr::var("n"), BinOp::Add, Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::binop(Expr::var("n"), BinOp::Add, Expr::int_lit(1))),
        "a by-ref argument's fact must die at the call"
    );
}

/// An unfolded array read (`$a[0]`) may warn at runtime but writes no locals:
/// scalar facts survive it.
#[test]
fn test_array_read_keeps_facts() {
    let program = vec![
        Stmt::assign("x", Expr::int_lit(5)),
        Stmt::assign(
            "y",
            Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(Expr::var("a")),
                    index: Box::new(Expr::int_lit(0)),
                },
                Span::dummy(),
            ),
        ),
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::int_lit(6)),
        "an array read cannot write locals"
    );
}

/// `unset($a[0])` invalidates only `$a`; unrelated facts keep folding.
#[test]
fn test_unset_array_element_keeps_unrelated_facts() {
    let program = vec![
        Stmt::assign("x", Expr::int_lit(5)),
        call_stmt(
            "unset",
            vec![Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(Expr::var("a")),
                    index: Box::new(Expr::int_lit(0)),
                },
                Span::dummy(),
            )],
        ),
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::int_lit(6)),
        "unset($a[0]) writes only $a"
    );
}

/// A loop body containing a call keeps pre-loop facts for variables the body
/// cannot write.
#[test]
fn test_loop_with_call_keeps_unwritten_facts() {
    let program = vec![
        noisy_function("noisy"),
        Stmt::assign("x", Expr::int_lit(5)),
        Stmt::assign("i", Expr::int_lit(0)),
        Stmt::new(
            StmtKind::While {
                condition: Expr::binop(Expr::var("i"), BinOp::Lt, Expr::int_lit(3)),
                body: vec![
                    call_stmt("noisy", vec![Expr::var("i")]),
                    Stmt::assign(
                        "i",
                        Expr::binop(Expr::var("i"), BinOp::Add, Expr::int_lit(1)),
                    ),
                ],
            },
            Span::dummy(),
        ),
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[4],
        Stmt::echo(Expr::int_lit(6)),
        "the loop writes $i, not $x"
    );
}

/// An echo whose expression embeds a call still keeps unrelated facts.
#[test]
fn test_echo_with_embedded_call_keeps_facts() {
    let program = vec![
        noisy_function("noisy"),
        Stmt::assign("x", Expr::int_lit(5)),
        Stmt::echo(Expr::new(
            ExprKind::FunctionCall {
                name: Name::from("noisy"),
                args: vec![Expr::var("y")],
            },
            Span::dummy(),
        )),
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[3],
        Stmt::echo(Expr::int_lit(6)),
        "echoing a call result cannot write caller locals"
    );
}

/// A callback-invoking builtin (`call_user_func`) forwards caller arguments to
/// arbitrary user code, which may take them by reference: the argument's fact
/// must die at the call.
#[test]
fn test_callback_builtin_exposes_forwarded_arguments() {
    let program = vec![
        Stmt::assign("v", Expr::int_lit(5)),
        call_stmt("call_user_func", vec![Expr::var("cb"), Expr::var("v")]),
        Stmt::echo(Expr::binop(Expr::var("v"), BinOp::Add, Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::binop(Expr::var("v"), BinOp::Add, Expr::int_lit(1))),
        "call_user_func may pass $v to a by-ref parameter"
    );
}

/// Creating a closure with a by-ref capture kills the captured variable's
/// existing fact — the volatility mark alone only blocks future facts.
#[test]
fn test_closure_by_ref_capture_kills_existing_fact() {
    let closure = Expr::new(
        ExprKind::Closure {
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            body: Vec::new(),
            is_arrow: false,
            is_static: false,
            by_ref_return: false,
            captures: vec!["x".to_string()],
            capture_refs: vec!["x".to_string()],
        },
        Span::dummy(),
    );
    let program = vec![
        Stmt::assign("x", Expr::int_lit(5)),
        Stmt::assign("c", closure),
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
        "the pre-capture fact must die at the closure creation site"
    );
}

/// An expression-position assignment through a nested lvalue chain
/// (`$x = ($a[0][1] = "X")`) writes `$a` even though the chain's root is not
/// the immediate array operand: the array fact must die. Regression for the
/// fast-path write collector missing nested lvalue roots.
#[test]
fn test_nested_lvalue_assignment_expr_kills_array_fact() {
    let nested_target = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(Expr::var("a")),
                    index: Box::new(Expr::int_lit(0)),
                },
                Span::dummy(),
            )),
            index: Box::new(Expr::int_lit(1)),
        },
        Span::dummy(),
    );
    let program = vec![
        Stmt::assign(
            "a",
            Expr::new(
                ExprKind::ArrayLiteral(vec![Expr::string_lit("ab"), Expr::string_lit("cd")]),
                Span::dummy(),
            ),
        ),
        Stmt::assign(
            "x",
            Expr::new(
                ExprKind::Assignment {
                    target: Box::new(nested_target),
                    value: Box::new(Expr::string_lit("X")),
                    result_target: None,
                    prelude: Vec::new(),
                    conditional_value_temp: None,
                },
                Span::dummy(),
            ),
        ),
        Stmt::echo(Expr::new(
            ExprKind::ArrayAccess {
                array: Box::new(Expr::var("a")),
                index: Box::new(Expr::int_lit(0)),
            },
            Span::dummy(),
        )),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::new(
            ExprKind::ArrayAccess {
                array: Box::new(Expr::var("a")),
                index: Box::new(Expr::int_lit(0)),
            },
            Span::dummy(),
        )),
        "a string-offset write through $a[0][1] mutates $a; the fact must die"
    );
}

/// `ptr($x)` takes the address of a local: any later `ptr_set` through any
/// alias of that pointer rewrites `$x` invisibly, so `$x` must never carry a
/// fact from the exposure point on. Regression for the elephc pointer
/// extension bypassing the PHP reference model.
#[test]
fn test_ptr_address_of_kills_and_volatilizes_fact() {
    let program = vec![
        Stmt::assign("x", Expr::int_lit(21)),
        Stmt::assign(
            "p",
            Expr::new(
                ExprKind::FunctionCall {
                    name: Name::from("ptr"),
                    args: vec![Expr::var("x")],
                },
                Span::dummy(),
            ),
        ),
        call_stmt("ptr_set", vec![Expr::var("p"), Expr::int_lit(42)]),
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
    ];

    let propagated = propagate_constants(program);

    assert_eq!(
        propagated[3],
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
        "an address-taken local must never carry a propagated fact"
    );
}
