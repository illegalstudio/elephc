//! Purpose:
//! Implements Pratt parsing for PHP infix, postfix, access, call, and assignment expressions.
//! Encodes operator precedence and associativity into binding-power tables.
//!
//! Called from:
//! - `crate::parser::expr::parse_expr()` and recursive expression parsing paths.
//!
//! Key details:
//! - Binding powers must match PHP precedence exactly because downstream passes trust the AST shape.

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::names::Name;
use crate::parser::ast::{BinOp, CallableTarget, Expr, ExprKind, InstanceOfTarget};
use crate::parser::stmt::parse_name;
use crate::span::Span;

use super::assignment_targets::{
    AssignmentExpressionLowerer,
    assignment_value_may_mutate_target_dependency, is_assignment_expression_target,
    is_non_local_assignment_target,
};
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
            Token::Arrow | Token::QuestionArrow => {
                let arrow_span = tokens[*pos].1;
                let nullsafe = tokens[*pos].0 == Token::QuestionArrow;
                *pos += 1;
                let member_name = match tokens.get(*pos).map(|(token, _)| token) {
                    Some(Token::Identifier(name)) => {
                        let name = name.clone();
                        *pos += 1;
                        name
                    }
                    // PHP 7+ allows reserved keywords as method/property names after `->`.
                    // Whitelist the keywords known to appear as method names in built-in
                    // and library APIs (e.g. `Fiber::throw`, `Generator::throw`).
                    Some(Token::Throw) => {
                        *pos += 1;
                        "throw".to_string()
                    }
                    Some(Token::Yield) => {
                        *pos += 1;
                        "yield".to_string()
                    }
                    Some(Token::Match) => {
                        *pos += 1;
                        "match".to_string()
                    }
                    Some(Token::Print) => {
                        *pos += 1;
                        "print".to_string()
                    }
                    Some(Token::Echo) => {
                        *pos += 1;
                        "echo".to_string()
                    }
                    Some(Token::Return) => {
                        *pos += 1;
                        "return".to_string()
                    }
                    _ => {
                        return Err(CompileError::new(
                            arrow_span,
                            if nullsafe {
                                "Expected property or method name after '?->'"
                            } else {
                                "Expected property or method name after '->'"
                            },
                        ))
                    }
                };
                if *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
                    *pos += 1;
                    if parse_first_class_callable_parens(tokens, pos)? {
                        if nullsafe {
                            return Err(CompileError::new(
                                arrow_span,
                                "Cannot combine nullsafe operator with Closure creation",
                            ));
                        }
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
                            if nullsafe {
                                ExprKind::NullsafeMethodCall {
                                    object: Box::new(lhs),
                                    method: member_name,
                                    args,
                                }
                            } else {
                                ExprKind::MethodCall {
                                    object: Box::new(lhs),
                                    method: member_name,
                                    args,
                                }
                            },
                            arrow_span,
                        );
                    }
                } else {
                    lhs = Expr::new(
                        if nullsafe {
                            ExprKind::NullsafePropertyAccess {
                                object: Box::new(lhs),
                                property: member_name,
                            }
                        } else {
                            ExprKind::PropertyAccess {
                                object: Box::new(lhs),
                                property: member_name,
                            }
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

        if tokens[*pos].0 == Token::InstanceOf {
            let instanceof_bp = 35;
            if instanceof_bp < min_bp {
                break;
            }

            let span = tokens[*pos].1;
            *pos += 1;
            let target = parse_instanceof_target(tokens, pos, span)?;
            lhs = Expr::new(
                ExprKind::InstanceOf {
                    value: Box::new(lhs),
                    target,
                },
                span,
            );
            continue;
        }

        if let Some((op, l_bp, r_bp)) = assignment_bp(&tokens[*pos].0) {
            if l_bp < min_bp {
                break;
            }

            if !is_assignment_expression_target(&lhs) {
                return Err(CompileError::new(lhs.span, "Invalid assignment target"));
            }

            let span = tokens[*pos].1;
            *pos += 1;
            let rhs = parse_expr_bp(tokens, pos, r_bp)?;
            if is_non_local_assignment_target(&lhs) {
                let null_coalesce_assign = matches!(op, AssignmentOperator::NullCoalesce);
                let needs_conditional_value_temp =
                    null_coalesce_assign && assignment_value_may_mutate_target_dependency(&lhs, &rhs);

                let mut lowerer = AssignmentExpressionLowerer::new(span);
                let target = lowerer.stabilize_non_local_target(lhs, &rhs);
                let conditional_value_temp = needs_conditional_value_temp
                    .then(|| lowerer.reserve_value_temp());
                let rhs = if null_coalesce_assign {
                    rhs
                } else {
                    lowerer.bind_value(rhs)
                };
                let value = assignment_value(target.clone(), op, rhs, span);
                let prelude = lowerer.finish();
                lhs = Expr::new(
                    ExprKind::Assignment {
                        target: Box::new(target.clone()),
                        value: Box::new(value),
                        result_target: Some(Box::new(target)),
                        prelude,
                        conditional_value_temp,
                    },
                    span,
                );
            } else {
                let value = assignment_value(lhs.clone(), op, rhs, span);
                lhs = Expr::new(
                    ExprKind::Assignment {
                        target: Box::new(lhs),
                        value: Box::new(value),
                        result_target: None,
                        prelude: Vec::new(),
                        conditional_value_temp: None,
                    },
                    span,
                );
            }
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

#[derive(Debug, Clone, PartialEq)]
enum AssignmentOperator {
    Assign,
    Compound(BinOp),
    NullCoalesce,
}

fn assignment_bp(token: &Token) -> Option<(AssignmentOperator, u8, u8)> {
    let op = match token {
        Token::Assign => AssignmentOperator::Assign,
        Token::PlusAssign => AssignmentOperator::Compound(BinOp::Add),
        Token::MinusAssign => AssignmentOperator::Compound(BinOp::Sub),
        Token::StarAssign => AssignmentOperator::Compound(BinOp::Mul),
        Token::StarStarAssign => AssignmentOperator::Compound(BinOp::Pow),
        Token::SlashAssign => AssignmentOperator::Compound(BinOp::Div),
        Token::PercentAssign => AssignmentOperator::Compound(BinOp::Mod),
        Token::DotAssign => AssignmentOperator::Compound(BinOp::Concat),
        Token::AmpAssign => AssignmentOperator::Compound(BinOp::BitAnd),
        Token::PipeAssign => AssignmentOperator::Compound(BinOp::BitOr),
        Token::CaretAssign => AssignmentOperator::Compound(BinOp::BitXor),
        Token::LessLessAssign => AssignmentOperator::Compound(BinOp::ShiftLeft),
        Token::GreaterGreaterAssign => AssignmentOperator::Compound(BinOp::ShiftRight),
        Token::QuestionQuestionAssign => AssignmentOperator::NullCoalesce,
        _ => return None,
    };
    Some((op, 7, 6))
}

fn assignment_value(target: Expr, op: AssignmentOperator, rhs: Expr, span: Span) -> Expr {
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

fn parse_instanceof_target(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<InstanceOfTarget, CompileError> {
    match tokens.get(*pos).map(|(token, _)| token) {
        Some(Token::Self_) => {
            *pos += 1;
            Ok(InstanceOfTarget::Name(Name::unqualified("self")))
        }
        Some(Token::Parent) => {
            *pos += 1;
            Ok(InstanceOfTarget::Name(Name::unqualified("parent")))
        }
        Some(Token::Static) => {
            *pos += 1;
            Ok(InstanceOfTarget::Name(Name::unqualified("static")))
        }
        Some(Token::Variable(_)) | Some(Token::LParen) => {
            let target = parse_expr_bp(tokens, pos, 36)?;
            Ok(InstanceOfTarget::Expr(Box::new(target)))
        }
        _ => parse_name(
            tokens,
            pos,
            span,
            "Expected class or interface name after 'instanceof'",
        )
        .map(InstanceOfTarget::Name),
    }
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
