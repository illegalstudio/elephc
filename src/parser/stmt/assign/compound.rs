use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::parser::expr::parse_assignment_value_expr;
use crate::span::Span;

use super::super::expect_semicolon;

pub(super) fn parse_assign(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let name = match &tokens[*pos].0 {
        Token::Variable(n) => n.clone(),
        _ => unreachable!(),
    };
    *pos += 1;

    if *pos >= tokens.len() {
        return Err(CompileError::new(span, "Expected '=' after variable name"));
    }

    use crate::parser::ast::BinOp;
    let compound_op = match &tokens[*pos].0 {
        Token::PlusAssign => Some(BinOp::Add),
        Token::MinusAssign => Some(BinOp::Sub),
        Token::StarAssign => Some(BinOp::Mul),
        Token::StarStarAssign => Some(BinOp::Pow),
        Token::SlashAssign => Some(BinOp::Div),
        Token::PercentAssign => Some(BinOp::Mod),
        Token::DotAssign => Some(BinOp::Concat),
        Token::AmpAssign => Some(BinOp::BitAnd),
        Token::PipeAssign => Some(BinOp::BitOr),
        Token::CaretAssign => Some(BinOp::BitXor),
        Token::LessLessAssign => Some(BinOp::ShiftLeft),
        Token::GreaterGreaterAssign => Some(BinOp::ShiftRight),
        Token::Assign => None,
        Token::QuestionQuestionAssign => {
            *pos += 1;
            let rhs = parse_assignment_value_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            let value = Expr::new(
                ExprKind::NullCoalesce {
                    value: Box::new(Expr::new(ExprKind::Variable(name.clone()), span)),
                    default: Box::new(rhs),
                },
                span,
            );
            return Ok(Stmt::new(StmtKind::Assign { name, value }, span));
        }
        _ => return Err(CompileError::new(span, "Expected '=' after variable name")),
    };
    *pos += 1;

    let rhs = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;

    let value = if let Some(op) = compound_op {
        Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::new(ExprKind::Variable(name.clone()), span)),
                op,
                right: Box::new(rhs),
            },
            span,
        )
    } else {
        rhs
    };

    Ok(Stmt::new(StmtKind::Assign { name, value }, span))
}

