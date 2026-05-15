//! Purpose:
//! Parser test root wiring and shared AST assertion helpers for PHP syntax coverage.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Helpers parse inline PHP snippets, assert failures, and inspect literals or promoted assignments.

use elephc::lexer::tokenize;
use elephc::names::Name;
use elephc::parser::ast::{
    BinOp, CallableTarget, CatchClause, Expr, ExprKind, MagicConstant, StaticReceiver, Stmt,
    StmtKind, TraitAdaptation, TypeExpr, UseKind, Visibility,
};
use elephc::parser::parse;

fn parse_source(src: &str) -> Vec<Stmt> {
    let tokens = tokenize(src).unwrap();
    parse(&tokens).unwrap()
}

fn parse_fails(src: &str) -> bool {
    let tokens = match tokenize(src) {
        Ok(t) => t,
        Err(_) => return true,
    };
    parse(&tokens).is_err()
}

fn assert_path_string_literal(path: &Expr, expected: &str) {
    match &path.kind {
        ExprKind::StringLiteral(s) => assert_eq!(s, expected),
        other => panic!("expected StringLiteral path, got {:?}", other),
    }
}

fn assert_promoted_assignment(stmt: &Stmt, expected: &str) {
    match &stmt.kind {
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => {
            assert_eq!(object.kind, ExprKind::This);
            assert_eq!(property, expected);
            assert_eq!(value.kind, ExprKind::Variable(expected.into()));
        }
        other => panic!("Expected promoted property assignment, got {:?}", other),
    }
}

fn echoed_expr(stmts: &[Stmt]) -> &ExprKind {
    match &stmts[0].kind {
        StmtKind::Echo(expr) => &expr.kind,
        other => panic!("Expected echo stmt, got {:?}", other),
    }
}

#[path = "parser_tests/statements.rs"]
mod statements;
#[path = "parser_tests/expressions.rs"]
mod expressions;
#[path = "parser_tests/control.rs"]
mod control;
#[path = "parser_tests/includes.rs"]
mod includes;
#[path = "parser_tests/functions.rs"]
mod functions;
#[path = "parser_tests/classes.rs"]
mod classes;
#[path = "parser_tests/namespaces.rs"]
mod namespaces;
#[path = "parser_tests/exceptions.rs"]
mod exceptions;
#[path = "parser_tests/declarations.rs"]
mod declarations;
#[path = "parser_tests/extensions.rs"]
mod extensions;
#[path = "parser_tests/magic_constants.rs"]
mod magic_constants;
#[path = "parser_tests/never.rs"]
mod never;
#[path = "parser_tests/attributes.rs"]
mod attributes;
#[path = "parser_tests/yield_parsing.rs"]
mod yield_parsing;
