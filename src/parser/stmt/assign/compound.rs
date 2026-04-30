use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind};
use crate::parser::expr::{parse_assignment_value_expr, parse_expr};
use crate::span::Span;

use super::super::expect_semicolon;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum AssignmentOperator {
    Assign,
    Compound(BinOp),
    NullCoalesce,
}

pub(super) fn parse_assign(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let start = *pos;
    let name = match &tokens[*pos].0 {
        Token::Variable(n) => n.clone(),
        _ => unreachable!(),
    };
    *pos += 1;

    if *pos >= tokens.len() {
        return Err(CompileError::new(span, "Expected '=' after variable name"));
    }

    let op = assignment_operator(&tokens[*pos].0)
        .ok_or_else(|| CompileError::new(span, "Expected '=' after variable name"))?;
    *pos += 1;

    let rhs = parse_assignment_value_expr(tokens, pos)?;
    if matches!(
        tokens.get(*pos).map(|(token, _)| token),
        Some(Token::And | Token::Or | Token::Xor)
    ) {
        *pos = start;
        let expr = parse_expr(tokens, pos)?;
        expect_semicolon(tokens, pos)?;
        return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
    }
    expect_semicolon(tokens, pos)?;

    let target = Expr::new(ExprKind::Variable(name.clone()), span);
    let value = assignment_value(target, op, rhs, span);

    Ok(Stmt::new(StmtKind::Assign { name, value }, span))
}

pub(super) fn assignment_operator(token: &Token) -> Option<AssignmentOperator> {
    match token {
        Token::Assign => Some(AssignmentOperator::Assign),
        Token::PlusAssign => Some(AssignmentOperator::Compound(BinOp::Add)),
        Token::MinusAssign => Some(AssignmentOperator::Compound(BinOp::Sub)),
        Token::StarAssign => Some(AssignmentOperator::Compound(BinOp::Mul)),
        Token::StarStarAssign => Some(AssignmentOperator::Compound(BinOp::Pow)),
        Token::SlashAssign => Some(AssignmentOperator::Compound(BinOp::Div)),
        Token::PercentAssign => Some(AssignmentOperator::Compound(BinOp::Mod)),
        Token::DotAssign => Some(AssignmentOperator::Compound(BinOp::Concat)),
        Token::AmpAssign => Some(AssignmentOperator::Compound(BinOp::BitAnd)),
        Token::PipeAssign => Some(AssignmentOperator::Compound(BinOp::BitOr)),
        Token::CaretAssign => Some(AssignmentOperator::Compound(BinOp::BitXor)),
        Token::LessLessAssign => Some(AssignmentOperator::Compound(BinOp::ShiftLeft)),
        Token::GreaterGreaterAssign => Some(AssignmentOperator::Compound(BinOp::ShiftRight)),
        Token::QuestionQuestionAssign => Some(AssignmentOperator::NullCoalesce),
        _ => None,
    }
}

pub(super) fn assignment_value(
    target: Expr,
    op: AssignmentOperator,
    rhs: Expr,
    span: Span,
) -> Expr {
    match op {
        AssignmentOperator::Assign => rhs,
        AssignmentOperator::Compound(op) => Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(target),
                op,
                right: Box::new(rhs),
            },
            span,
        ),
        AssignmentOperator::NullCoalesce => Expr::new(
            ExprKind::NullCoalesce {
                value: Box::new(target),
                default: Box::new(rhs),
            },
            span,
        ),
    }
}
