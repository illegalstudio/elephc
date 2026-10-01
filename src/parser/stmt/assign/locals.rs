//! Purpose:
//! Parses local variable statement forms beyond ordinary assignment.
//! Handles increment/decrement statements, global declarations, static locals, and typed assignments.
//!
//! Called from:
//! - `crate::parser::stmt::assign::simple::parse_variable_stmt()` and statement dispatch.
//!
//! Key details:
//! - Typed local syntax is a parser-level distinction that later passes use for declaration semantics.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::parser::expr::parse_assignment_value_expr;
use crate::span::Span;

use super::super::params::parse_type_expr;
use super::super::{expect_semicolon, expect_token};

/// Handle ++$var; or --$var; as standalone statements. Also handles ++A::$x and --A::$x
/// (prefix increment/decrement on static properties) by desugaring to a compound assignment.
pub(in crate::parser::stmt) fn parse_incdec_stmt(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let is_increment = tokens[*pos].0 == Token::PlusPlus;
    *pos += 1;

    let next_token = tokens.get(*pos).map(|(t, _)| t);
    let is_scoped = matches!(
        next_token,
        Some(Token::Identifier(_)) | Some(Token::Self_) | Some(Token::Parent) | Some(Token::Static)
    ) && tokens.get(*pos + 1).map(|(t, _)| t) == Some(&Token::DoubleColon)
        && tokens.get(*pos + 2).map(|(t, _)| t).is_some_and(|t| matches!(t, Token::Variable(_)));

    if is_scoped {
        let lhs_expr = crate::parser::expr::parse_expr(tokens, pos)
            .map_err(|_| CompileError::new(span, "Expected variable after '++'"))?;
        expect_semicolon(tokens, pos)?;
        return super::postfix::lower_postfix_incdec_assignment(lhs_expr, is_increment, span);
    }

    // `++$this->n;`, `++$obj->n;`, and `++$a[0];` target storage the simple local path
    // cannot name. Statement position discards the operator's value, so they lower
    // through the same read-modify-write shape as their postfix spellings.
    if starts_complex_incdec_target(tokens, *pos) {
        let lhs_expr = crate::parser::expr::parse_expr(tokens, pos)?;
        expect_semicolon(tokens, pos)?;
        return super::postfix::lower_postfix_incdec_assignment(lhs_expr, is_increment, span);
    }

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Variable(n)) => n.clone(),
        _ => {
            let op = if is_increment { "++" } else { "--" };
            return Err(CompileError::new(
                span,
                &format!("Expected variable after '{}'", op),
            ));
        }
    };
    *pos += 1;
    expect_semicolon(tokens, pos)?;

    let kind = if is_increment {
        ExprKind::PreIncrement(name)
    } else {
        ExprKind::PreDecrement(name)
    };
    let expr = Expr::new(kind, span);
    Ok(Stmt::new(StmtKind::ExprStmt(expr), span))
}

/// Returns true when the tokens after a prefix `++`/`--` name a property, array element,
/// or `$this` member rather than a plain local variable.
///
/// `$this` always continues into a member access, and a variable is only a complex target
/// when it is followed by `->`, `?->`, or `[`. Everything else keeps the plain
/// `PreIncrement`/`PreDecrement` local path.
fn starts_complex_incdec_target(tokens: &[SpannedToken], pos: usize) -> bool {
    match tokens.get(pos).map(|(token, _)| token) {
        Some(Token::This) => true,
        Some(Token::Variable(_)) => matches!(
            tokens.get(pos + 1).map(|(token, _)| token),
            Some(Token::Arrow) | Some(Token::QuestionArrow) | Some(Token::LBracket)
        ),
        _ => false,
    }
}

/// Parses a `global $var, ...;` declaration statement.
/// Consumes the `global` keyword, then collects a comma-separated list of variable names
/// until a semicolon. Returns a `StmtKind::Global` node.
pub(in crate::parser::stmt) fn parse_global(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'global'

    let mut vars = Vec::new();
    loop {
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Variable(n)) => {
                vars.push(n.clone());
                *pos += 1;
            }
            _ => return Err(CompileError::new(span, "Expected variable after 'global'")),
        }
        if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
            *pos += 1;
        } else {
            break;
        }
    }

    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::Global { vars }, span))
}

/// Parses a `static $var = expr;` or `static $var;` declaration statement.
/// Consumes the `static` keyword, then expects a single variable name optionally followed by
/// `=` and an initializer expression; a missing initializer desugars to `= null` (PHP treats
/// both forms identically). Returns a `StmtKind::StaticVar` node.
pub(in crate::parser::stmt) fn parse_static_var(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'static'

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Variable(n)) => n.clone(),
        _ => return Err(CompileError::new(span, "Expected variable after 'static'")),
    };
    *pos += 1;

    let init = if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Assign)) {
        *pos += 1;
        parse_assignment_value_expr(tokens, pos)?
    } else {
        Expr::new(ExprKind::Null, span)
    };
    expect_semicolon(tokens, pos)?;

    Ok(Stmt::new(StmtKind::StaticVar { name, init }, span))
}

/// Returns true if the token sequence at `pos` looks like a typed local assignment:
/// a type expression followed by a variable name. Performs a lookahead parse of the type
/// expression only; does not consume any tokens.
pub(in crate::parser::stmt) fn looks_like_typed_assign(tokens: &[SpannedToken], pos: usize) -> bool {
    let mut probe = pos;
    match parse_type_expr(tokens, &mut probe, tokens[pos].1.span) {
        Ok(_) => matches!(tokens.get(probe).map(|(t, _)| t), Some(Token::Variable(_))),
        Err(_) => false,
    }
}

/// Parses a typed local assignment: `Type $var = expr;`
/// Consumes a type expression, a variable name, the `=` token, and an initializer expression.
/// Returns a `StmtKind::TypedAssign` node.
pub(in crate::parser::stmt) fn parse_typed_assign(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let type_expr = parse_type_expr(tokens, pos, span)?;
    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Variable(name)) => {
            let name = name.clone();
            *pos += 1;
            name
        }
        _ => {
            return Err(CompileError::new(
                span,
                "Expected variable after type annotation",
            ))
        }
    };
    expect_token(
        tokens,
        pos,
        &Token::Assign,
        "Expected '=' after typed variable",
    )?;
    let value = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        },
        span,
    ))
}
