use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{BinOp, CallableTarget, Expr, ExprKind};
use crate::span::Span;

use super::calls::parse_first_class_callable_parens;
use super::parse_args;
use super::prefix::parse_prefix;
use super::parse_expr;

pub(super) fn parse_expr_bp(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    min_bp: u8,
) -> Result<Expr, CompileError> {
    let mut lhs = parse_prefix(tokens, pos)?;

    loop {
        if *pos >= tokens.len() {
            break;
        }

        match &tokens[*pos].0 {
            Token::LBracket => {
                let span = tokens[*pos].1;
                *pos += 1;
                let index = parse_expr(tokens, pos)?;
                if *pos >= tokens.len() || tokens[*pos].0 != Token::RBracket {
                    return Err(CompileError::new(span, "Expected ']'"));
                }
                *pos += 1;
                lhs = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(lhs),
                        index: Box::new(index),
                    },
                    span,
                );
            }
            Token::Arrow => {
                let arrow_span = tokens[*pos].1;
                *pos += 1;
                let member_name = match tokens.get(*pos).map(|(token, _)| token) {
                    Some(Token::Identifier(name)) => {
                        let name = name.clone();
                        *pos += 1;
                        name
                    }
                    _ => {
                        return Err(CompileError::new(
                            arrow_span,
                            "Expected property or method name after '->'",
                        ))
                    }
                };
                if *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
                    *pos += 1;
                    if parse_first_class_callable_parens(tokens, pos)? {
                        lhs = Expr::new(
                            ExprKind::FirstClassCallable(CallableTarget::Method {
                                object: Box::new(lhs),
                                method: member_name,
                            }),
                            arrow_span,
                        );
                    } else {
                        let args = parse_args(tokens, pos, arrow_span)?;
                        lhs = Expr::new(
                            ExprKind::MethodCall {
                                object: Box::new(lhs),
                                method: member_name,
                                args,
                            },
                            arrow_span,
                        );
                    }
                } else {
                    lhs = Expr::new(
                        ExprKind::PropertyAccess {
                            object: Box::new(lhs),
                            property: member_name,
                        },
                        arrow_span,
                    );
                }
            }
            Token::LParen => {
                if matches!(
                    lhs.kind,
                    ExprKind::ArrayAccess { .. }
                        | ExprKind::ExprCall { .. }
                        | ExprKind::ClosureCall { .. }
                        | ExprKind::FunctionCall { .. }
                ) {
                    let call_span = tokens[*pos].1;
                    *pos += 1;
                    let args = parse_args(tokens, pos, call_span)?;
                    lhs = Expr::new(
                        ExprKind::ExprCall {
                            callee: Box::new(lhs),
                            args,
                        },
                        call_span,
                    );
                } else {
                    break;
                }
            }
            _ => break,
        }
    }

    loop {
        if *pos >= tokens.len() {
            break;
        }

        let (op, l_bp, r_bp) = match infix_bp(&tokens[*pos].0) {
            Some(binding) => binding,
            None => break,
        };

        if l_bp < min_bp {
            break;
        }

        let span = tokens[*pos].1;
        *pos += 1;
        let rhs = parse_expr_bp(tokens, pos, r_bp)?;
        if op == BinOp::NullCoalesce {
            lhs = Expr::new(
                ExprKind::NullCoalesce {
                    value: Box::new(lhs),
                    default: Box::new(rhs),
                },
                span,
            );
        } else {
            lhs = Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(lhs),
                    op,
                    right: Box::new(rhs),
                },
                span,
            );
        }
    }

    if *pos < tokens.len() && tokens[*pos].0 == Token::Question && min_bp == 0 {
        let span = tokens[*pos].1;
        *pos += 1;
        let then_expr = parse_expr(tokens, pos)?;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::Colon {
            return Err(CompileError::new(span, "Expected ':' in ternary operator"));
        }
        *pos += 1;
        let else_expr = parse_expr_bp(tokens, pos, 0)?;
        lhs = Expr::new(
            ExprKind::Ternary {
                condition: Box::new(lhs),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
            },
            span,
        );
    }

    Ok(lhs)
}

fn infix_bp(token: &Token) -> Option<(BinOp, u8, u8)> {
    match token {
        Token::QuestionQuestion => Some((BinOp::NullCoalesce, 2, 1)),
        Token::OrOr => Some((BinOp::Or, 3, 4)),
        Token::AndAnd => Some((BinOp::And, 5, 6)),
        Token::Pipe => Some((BinOp::BitOr, 7, 8)),
        Token::Caret => Some((BinOp::BitXor, 9, 10)),
        Token::Ampersand => Some((BinOp::BitAnd, 11, 12)),
        Token::EqualEqual => Some((BinOp::Eq, 13, 14)),
        Token::NotEqual => Some((BinOp::NotEq, 13, 14)),
        Token::EqualEqualEqual => Some((BinOp::StrictEq, 13, 14)),
        Token::NotEqualEqual => Some((BinOp::StrictNotEq, 13, 14)),
        Token::Less => Some((BinOp::Lt, 15, 16)),
        Token::Greater => Some((BinOp::Gt, 15, 16)),
        Token::LessEqual => Some((BinOp::LtEq, 15, 16)),
        Token::GreaterEqual => Some((BinOp::GtEq, 15, 16)),
        Token::Spaceship => Some((BinOp::Spaceship, 15, 16)),
        Token::LessLess => Some((BinOp::ShiftLeft, 17, 18)),
        Token::GreaterGreater => Some((BinOp::ShiftRight, 17, 18)),
        Token::Dot => Some((BinOp::Concat, 19, 20)),
        Token::Plus => Some((BinOp::Add, 21, 22)),
        Token::Minus => Some((BinOp::Sub, 21, 22)),
        Token::Star => Some((BinOp::Mul, 23, 24)),
        Token::Slash => Some((BinOp::Div, 23, 24)),
        Token::Percent => Some((BinOp::Mod, 23, 24)),
        Token::StarStar => Some((BinOp::Pow, 29, 28)),
        _ => None,
    }
}
