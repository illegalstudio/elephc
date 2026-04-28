use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{ExprKind, Stmt, StmtKind};
use crate::parser::expr::{parse_assignment_value_expr, parse_expr};
use crate::span::Span;

use super::super::expect_semicolon;

pub(in crate::parser::stmt) fn try_parse_postfix_assignment(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Option<Stmt>, CompileError> {
    let start = *pos;
    let Some(assign_pos) = find_top_level_assign(tokens, start) else {
        return Ok(None);
    };
    if assign_pos < start + 3 {
        return Ok(None);
    }

    let lhs = &tokens[start..assign_pos];
    let is_append =
        lhs.len() >= 3 && lhs[lhs.len() - 2].0 == Token::LBracket && lhs[lhs.len() - 1].0 == Token::RBracket;
    let contains_postfix = lhs
        .iter()
        .skip(1)
        .any(|(token, _)| matches!(token, Token::Arrow | Token::QuestionArrow | Token::LBracket));
    if !contains_postfix {
        return Ok(None);
    }

    let mut lhs_pos = 0;
    let lhs_expr_tokens = if is_append { &lhs[..lhs.len() - 2] } else { lhs };
    let lhs_expr = parse_expr(lhs_expr_tokens, &mut lhs_pos)?;
    if lhs_pos != lhs_expr_tokens.len() {
        return Err(CompileError::new(span, "Invalid assignment target"));
    }

    *pos = assign_pos + 1;
    let value = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;

    let stmt = match lhs_expr.kind {
        ExprKind::Variable(array) if is_append => StmtKind::ArrayPush { array, value },
        ExprKind::PropertyAccess { object, property } if is_append => StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        },
        ExprKind::ArrayAccess { array, index } => {
            match array.kind {
                ExprKind::Variable(array) => StmtKind::ArrayAssign {
                    array,
                    index: *index,
                    value,
                },
                ExprKind::PropertyAccess { object, property } => StmtKind::PropertyArrayAssign {
                    object,
                    property,
                    index: *index,
                    value,
                },
                _ => return Err(CompileError::new(span, "Invalid assignment target")),
            }
        }
        ExprKind::PropertyAccess { object, property } => StmtKind::PropertyAssign {
            object,
            property,
            value,
        },
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };

    Ok(Some(Stmt::new(stmt, span)))
}

pub(in crate::parser::stmt) fn try_parse_scoped_property_assignment(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Option<Stmt>, CompileError> {
    let start = *pos;
    let Some(assign_pos) = find_top_level_assign(tokens, start) else {
        return Ok(None);
    };
    if assign_pos < start + 3 {
        return Ok(None);
    }

    let lhs = &tokens[start..assign_pos];
    let is_append =
        lhs.len() >= 3 && lhs[lhs.len() - 2].0 == Token::LBracket && lhs[lhs.len() - 1].0 == Token::RBracket;
    let mut lhs_pos = 0;
    let lhs_expr_tokens = if is_append { &lhs[..lhs.len() - 2] } else { lhs };
    let lhs_expr = parse_expr(lhs_expr_tokens, &mut lhs_pos)?;
    if lhs_pos != lhs_expr_tokens.len() {
        return Err(CompileError::new(span, "Invalid assignment target"));
    }

    *pos = assign_pos + 1;
    let value = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;

    let stmt = match lhs_expr.kind {
        ExprKind::StaticPropertyAccess { receiver, property } if is_append => {
            StmtKind::StaticPropertyArrayPush {
                receiver,
                property,
                value,
            }
        }
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::StaticPropertyAccess { receiver, property } => {
                StmtKind::StaticPropertyArrayAssign {
                    receiver,
                    property,
                    index: *index,
                    value,
                }
            }
            _ => return Err(CompileError::new(span, "Invalid assignment target")),
        },
        ExprKind::StaticPropertyAccess { receiver, property } => StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        },
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };

    Ok(Some(Stmt::new(stmt, span)))
}

fn find_top_level_assign(tokens: &[(Token, Span)], start: usize) -> Option<usize> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut pos = start;

    while pos < tokens.len() {
        match tokens[pos].0 {
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            Token::LBracket => bracket_depth += 1,
            Token::RBracket => bracket_depth = bracket_depth.saturating_sub(1),
            Token::LBrace => brace_depth += 1,
            Token::RBrace => brace_depth = brace_depth.saturating_sub(1),
            Token::Assign if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                return Some(pos);
            }
            Token::Semicolon if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                return None;
            }
            _ => {}
        }
        pos += 1;
    }

    None
}

