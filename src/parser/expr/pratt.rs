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

        if tokens[*pos].0 == Token::Question {
            let ternary_bp = 7;
            if ternary_bp < min_bp {
                break;
            }

            let span = tokens[*pos].1;
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos].0 == Token::Colon {
                *pos += 1;
                let default = parse_expr_bp(tokens, pos, ternary_bp)?;
                lhs = Expr::new(
                    ExprKind::ShortTernary {
                        value: Box::new(lhs),
                        default: Box::new(default),
                    },
                    span,
                );
                continue;
            }

            let then_expr = parse_expr(tokens, pos)?;
            if *pos >= tokens.len() || tokens[*pos].0 != Token::Colon {
                return Err(CompileError::new(span, "Expected ':' in ternary operator"));
            }
            *pos += 1;
            let else_expr = parse_expr_bp(tokens, pos, ternary_bp)?;
            lhs = Expr::new(
                ExprKind::Ternary {
                    condition: Box::new(lhs),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                },
                span,
            );
            continue;
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

    Ok(lhs)
}

fn infix_bp(token: &Token) -> Option<(BinOp, u8, u8)> {
    match token {
        Token::Or => Some((BinOp::Or, 1, 2)),
        Token::Xor => Some((BinOp::Xor, 3, 4)),
        Token::And => Some((BinOp::And, 5, 6)),
        Token::QuestionQuestion => Some((BinOp::NullCoalesce, 9, 8)),
        Token::OrOr => Some((BinOp::Or, 11, 12)),
        Token::AndAnd => Some((BinOp::And, 13, 14)),
        Token::Pipe => Some((BinOp::BitOr, 15, 16)),
        Token::Caret => Some((BinOp::BitXor, 17, 18)),
        Token::Ampersand => Some((BinOp::BitAnd, 19, 20)),
        Token::EqualEqual => Some((BinOp::Eq, 21, 22)),
        Token::NotEqual => Some((BinOp::NotEq, 21, 22)),
        Token::EqualEqualEqual => Some((BinOp::StrictEq, 21, 22)),
        Token::NotEqualEqual => Some((BinOp::StrictNotEq, 21, 22)),
        Token::Less => Some((BinOp::Lt, 23, 24)),
        Token::Greater => Some((BinOp::Gt, 23, 24)),
        Token::LessEqual => Some((BinOp::LtEq, 23, 24)),
        Token::GreaterEqual => Some((BinOp::GtEq, 23, 24)),
        Token::Spaceship => Some((BinOp::Spaceship, 23, 24)),
        Token::LessLess => Some((BinOp::ShiftLeft, 25, 26)),
        Token::GreaterGreater => Some((BinOp::ShiftRight, 25, 26)),
        Token::Dot => Some((BinOp::Concat, 27, 28)),
        Token::Plus => Some((BinOp::Add, 29, 30)),
        Token::Minus => Some((BinOp::Sub, 29, 30)),
        Token::Star => Some((BinOp::Mul, 31, 32)),
        Token::Slash => Some((BinOp::Div, 31, 32)),
        Token::Percent => Some((BinOp::Mod, 31, 32)),
        Token::StarStar => Some((BinOp::Pow, 37, 36)),
        _ => None,
    }
}
