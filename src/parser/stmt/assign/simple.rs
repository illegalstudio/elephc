use super::{compound, postfix};
use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::parser::expr::{parse_assignment_value_expr, parse_expr};
use crate::span::Span;

use super::super::expect_semicolon;

/// Handle statements starting with $variable: assignment, array ops, or post-increment.
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
        if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
            *pos += 1;
            let value = parse_assignment_value_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            if let ExprKind::PropertyAccess { object, property } = expr.kind {
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

