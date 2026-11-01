//! Purpose:
//! Parses php's variable variables (`$$name`, `${expr}`) into constructs the compiler already
//! supports, so a class file that uses one compiles instead of being handed to the interpreter.
//!
//! Called from:
//! - `crate::parser::expr::prefix` for a read, and `crate::parser::stmt` for `$$name = expr;`.
//!
//! Key details:
//! - A READ `$$name` becomes `get_defined_vars()[$name] ?? null`: the value of the local whose
//!   name `$name` holds, or null when there is none. php also warns `Undefined variable $x` for an
//!   unknown name; this spelling stays silent, which is the one difference.
//! - A WRITE `$$name = expr;` becomes `extract([$name => expr])`, php's own by-name local write.
//!   `extract()` already raises the eval barrier, which widens every local of the function, and
//!   that is exactly what a write that may reach any local requires.
//! - Only these two shapes are modelled. A compound assignment, a by-reference binding or an
//!   element write through a variable variable still reports a syntax error, so nothing is
//!   silently mis-lowered. Carbon's `Date::set()`, `Localization::translateTimeString()` and
//!   `CarbonInterval::__construct()` use exactly the two supported shapes.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::names::Name;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;

/// Parses the name operand after a bare `$`: `$$name` names the value of `$name`, `${expr}`
/// names the value of `expr`, and `$$$name` names the value of `$$name`.
///
/// `pos` points at the `Dollar` token on entry and just past the operand on return.
pub(crate) fn parse_name_operand(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    *pos += 1;
    match tokens.get(*pos).map(|(token, _)| token) {
        Some(Token::Variable(name)) => {
            let operand = Expr::new(ExprKind::Variable(name.clone()), tokens[*pos].1.span);
            *pos += 1;
            Ok(operand)
        }
        Some(Token::LBrace) => {
            *pos += 1;
            let operand = crate::parser::expr::parse_expr(tokens, pos)?;
            crate::parser::stmt::expect_token(tokens, pos, &Token::RBrace, "Expected '}' after a variable-variable name")?;
            Ok(operand)
        }
        Some(Token::Dollar) => {
            let inner = parse_name_operand(tokens, pos, span)?;
            Ok(read(inner, span))
        }
        _ => Err(CompileError::new(span, "Expected a variable name after '$'")),
    }
}

/// Builds the read of the local named by `name`: `get_defined_vars()[$name] ?? null`.
pub(crate) fn read(name: Expr, span: Span) -> Expr {
    let defined = Expr::new(
        ExprKind::FunctionCall { name: Name::unqualified("get_defined_vars"), args: Vec::new() },
        span,
    );
    Expr::new(
        ExprKind::NullCoalesce {
            value: Box::new(Expr::new(
                ExprKind::ArrayAccess { array: Box::new(defined), index: Box::new(name) },
                span,
            )),
            default: Box::new(Expr::new(ExprKind::Null, span)),
        },
        span,
    )
}

/// Parses a statement that starts with a bare `$`: `$$name = expr;` becomes
/// `extract([$name => expr]);`.
pub(crate) fn parse_assignment_statement(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let name = parse_name_operand(tokens, pos, span)?;
    if !matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::Assign)) {
        return Err(CompileError::new(
            span,
            "Variable variables (`$$name`) are only supported in a plain `$$name = value;` write or a read",
        ));
    }
    *pos += 1;
    let value = crate::parser::expr::parse_expr(tokens, pos)?;
    crate::parser::stmt::expect_semicolon(tokens, pos)?;
    let extract = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("extract"),
            args: vec![Expr::new(ExprKind::ArrayLiteralAssoc(vec![(name, value)]), span)],
        },
        span,
    );
    Ok(Stmt::new(StmtKind::ExprStmt(extract), span))
}
