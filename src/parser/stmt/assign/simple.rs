//! Purpose:
//! Parses statements that begin with a PHP variable token.
//! Routes variable syntax to compound assignment, postfix assignment, or expression-statement parsing.
//!
//! Called from:
//! - `crate::parser::stmt::parse_stmt()`.
//!
//! Key details:
//! - Variable-leading statements are ambiguous, so dispatch order protects assignment-specific syntax first.

use super::{compound, postfix};
use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::parser::expr::{parse_assignment_value_expr, parse_expr};
use crate::span::Span;

use super::super::expect_semicolon;

/// Parses statements that begin with a PHP variable token (`$name`).
///
/// Dispatches to postfix assignment, post-increment/decrement, property access with
/// compound assignment, closure calls, or regular/compound assignment based on the
/// token that follows the variable.
///
/// # Arguments
/// - `tokens` — the token stream
/// - `pos` — current position (mutated by parsing)
/// - `span` — source span of the statement
///
/// # Returns
/// `Stmt` with `StmtKind::PostIncrement`, `PostDecrement`, `PropertyAssign`,
/// `ExprStmt`, or compound/regular assignment variants.
///
/// # Panics
/// Unreachable if the first token is not `Token::Variable`.
pub(in crate::parser::stmt) fn parse_variable_stmt(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let name = match &tokens[*pos].0 {
        Token::Variable(n) => n.clone(),
        _ => unreachable!(),
    };

    if let Some(stmt) = postfix::try_parse_postfix_assignment(tokens, pos, span)? {
        return Ok(stmt);
    }

    // Post-increment/decrement
    if *pos + 1 < tokens.len() {
        match &tokens[*pos + 1].0 {
            Token::PlusPlus => {
                *pos += 2;
                expect_semicolon(tokens, pos)?;
                let expr = Expr::new(ExprKind::PostIncrement(name), span);
                return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
            }
            Token::MinusMinus => {
                *pos += 2;
                expect_semicolon(tokens, pos)?;
                let expr = Expr::new(ExprKind::PostDecrement(name), span);
                return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
            }
            _ => {}
        }
    }

    if *pos + 1 < tokens.len()
        && matches!(
            tokens[*pos + 1].0,
            Token::Arrow | Token::QuestionArrow | Token::LBracket
        )
    {
        let expr = parse_expr(tokens, pos)?;
        if let Some(op) = tokens
            .get(*pos)
            .and_then(|(token, _)| compound::assignment_operator(token))
        {
            *pos += 1;
            let rhs = parse_assignment_value_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            if let ExprKind::PropertyAccess { object, property } = expr.kind {
                let target = Expr::new(
                    ExprKind::PropertyAccess {
                        object: object.clone(),
                        property: property.clone(),
                    },
                    span,
                );
                let value = compound::assignment_value(target, op, rhs, span);
                return Ok(Stmt::new(
                    StmtKind::PropertyAssign {
                        object,
                        property,
                        value,
                    },
                    span,
                ));
            }
            return Err(CompileError::new(span, "Invalid assignment target"));
        }
        expect_semicolon(tokens, pos)?;
        return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
    }

    // Closure call: $fn(args);
    if *pos + 1 < tokens.len() && tokens[*pos + 1].0 == Token::LParen {
        let expr = parse_expr(tokens, pos)?;
        expect_semicolon(tokens, pos)?;
        return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
    }

    // Regular or compound assignment
    compound::parse_assign(tokens, pos, span)
}
