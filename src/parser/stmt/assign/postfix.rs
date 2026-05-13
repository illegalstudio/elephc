//! Purpose:
//! Detects and lowers postfix assignments for complex expression targets.
//! Replays parseable l-values and creates effect-preserving lowerings for property/static assignments.
//!
//! Called from:
//! - `crate::parser::stmt::simple::parse_expr_stmt()` and assignment statement dispatch.
//!
//! Key details:
//! - Complex target lowering must not duplicate side effects while preserving PHP source evaluation order.

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind};
use crate::parser::expr::{parse_assignment_value_expr, parse_expr};
use crate::span::Span;

use super::super::expect_semicolon;
use super::compound::{assignment_operator, assignment_value, AssignmentOperator};

pub(in crate::parser::stmt) fn try_parse_postfix_assignment(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Option<Stmt>, CompileError> {
    let start = *pos;
    let Some((assign_pos, op)) = find_top_level_assignment(tokens, start) else {
        return Ok(None);
    };
    if assign_pos < start + 3 {
        return Ok(None);
    }

    let lhs = &tokens[start..assign_pos];
    let is_append =
        lhs.len() >= 3 && lhs[lhs.len() - 2].0 == Token::LBracket && lhs[lhs.len() - 1].0 == Token::RBracket;
    if is_append && op != AssignmentOperator::Assign {
        return Err(CompileError::new(span, "Invalid assignment target"));
    }
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
    let rhs = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    if op != AssignmentOperator::Assign && !can_replay_assignment_target(&lhs_expr) {
        return lower_effectful_postfix_assignment(lhs_expr, op, rhs, span).map(Some);
    }
    let value = assignment_value(lhs_expr.clone(), op, rhs, span);

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
    let Some((assign_pos, op)) = find_top_level_assignment(tokens, start) else {
        return Ok(None);
    };
    if assign_pos < start + 3 {
        return Ok(None);
    }

    let lhs = &tokens[start..assign_pos];
    let is_append =
        lhs.len() >= 3 && lhs[lhs.len() - 2].0 == Token::LBracket && lhs[lhs.len() - 1].0 == Token::RBracket;
    if is_append && op != AssignmentOperator::Assign {
        return Err(CompileError::new(span, "Invalid assignment target"));
    }
    let mut lhs_pos = 0;
    let lhs_expr_tokens = if is_append { &lhs[..lhs.len() - 2] } else { lhs };
    let lhs_expr = parse_expr(lhs_expr_tokens, &mut lhs_pos)?;
    if lhs_pos != lhs_expr_tokens.len() {
        return Err(CompileError::new(span, "Invalid assignment target"));
    }

    *pos = assign_pos + 1;
    let rhs = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    if op != AssignmentOperator::Assign && !can_replay_assignment_target(&lhs_expr) {
        return lower_effectful_static_assignment(lhs_expr, op, rhs, span).map(Some);
    }
    let value = assignment_value(lhs_expr.clone(), op, rhs, span);

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

fn find_top_level_assignment(
    tokens: &[(Token, Span)],
    start: usize,
) -> Option<(usize, AssignmentOperator)> {
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
            Token::Semicolon if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                return None;
            }
            _ if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                if let Some(op) = assignment_operator(&tokens[pos].0) {
                    return Some((pos, op));
                }
            }
            _ => {}
        }
        pos += 1;
    }

    None
}

pub(crate) fn can_replay_assignment_target(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) | ExprKind::This | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, index } => {
            can_replay_assignment_target(array) && can_replay_assignment_target(index)
        }
        ExprKind::PropertyAccess { object, .. } => can_replay_assignment_target(object),
        ExprKind::BinaryOp { left, right, .. } => {
            can_replay_assignment_target(left) && can_replay_assignment_target(right)
        }
        ExprKind::InstanceOf { value, target } => {
            can_replay_assignment_target(value) && can_replay_instanceof_target(target)
        }
        ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Cast { expr: value, .. }
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::NamedArg { value, .. }
        | ExprKind::Spread(value) => can_replay_assignment_target(value),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            can_replay_assignment_target(value) && can_replay_assignment_target(default)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            can_replay_assignment_target(condition)
                && can_replay_assignment_target(then_expr)
                && can_replay_assignment_target(else_expr)
        }
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::ClassConstant { .. } | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => true,
        _ => false,
    }
}

fn can_replay_instanceof_target(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(_) => true,
        InstanceOfTarget::Expr(expr) => can_replay_assignment_target(expr),
    }
}

fn lower_effectful_postfix_assignment(
    lhs_expr: Expr,
    op: AssignmentOperator,
    rhs: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    let mut lowerer = EffectfulTargetLowerer::new(span);
    let lowered = match lhs_expr.kind {
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::Variable(array) => {
                let index = lowerer.stabilize(*index);
                let target = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(Expr::new(ExprKind::Variable(array.clone()), span)),
                        index: Box::new(index.clone()),
                    },
                    span,
                );
                let value = assignment_value(target, op, rhs, span);
                StmtKind::ArrayAssign { array, index, value }
            }
            ExprKind::PropertyAccess { object, property } => {
                let object = Box::new(lowerer.stabilize(*object));
                let index = lowerer.stabilize(*index);
                let target = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(Expr::new(
                            ExprKind::PropertyAccess {
                                object: object.clone(),
                                property: property.clone(),
                            },
                            span,
                        )),
                        index: Box::new(index.clone()),
                    },
                    span,
                );
                let value = assignment_value(target, op, rhs, span);
                StmtKind::PropertyArrayAssign {
                    object,
                    property,
                    index,
                    value,
                }
            }
            _ => return Err(CompileError::new(span, "Invalid assignment target")),
        },
        ExprKind::PropertyAccess { object, property } => {
            let object = Box::new(lowerer.stabilize(*object));
            let target = Expr::new(
                ExprKind::PropertyAccess {
                    object: object.clone(),
                    property: property.clone(),
                },
                span,
            );
            let value = assignment_value(target, op, rhs, span);
            StmtKind::PropertyAssign {
                object,
                property,
                value,
            }
        }
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };
    Ok(lowerer.finish(lowered))
}

fn lower_effectful_static_assignment(
    lhs_expr: Expr,
    op: AssignmentOperator,
    rhs: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    let mut lowerer = EffectfulTargetLowerer::new(span);
    let lowered = match lhs_expr.kind {
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::StaticPropertyAccess { receiver, property } => {
                let index = lowerer.stabilize(*index);
                let target = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(Expr::new(
                            ExprKind::StaticPropertyAccess {
                                receiver: receiver.clone(),
                                property: property.clone(),
                            },
                            span,
                        )),
                        index: Box::new(index.clone()),
                    },
                    span,
                );
                let value = assignment_value(target, op, rhs, span);
                StmtKind::StaticPropertyArrayAssign {
                    receiver,
                    property,
                    index,
                    value,
                }
            }
            _ => return Err(CompileError::new(span, "Invalid assignment target")),
        },
        ExprKind::StaticPropertyAccess { receiver, property } => {
            let target = Expr::new(
                ExprKind::StaticPropertyAccess {
                    receiver: receiver.clone(),
                    property: property.clone(),
                },
                span,
            );
            let value = assignment_value(target, op, rhs, span);
            StmtKind::StaticPropertyAssign {
                receiver,
                property,
                value,
            }
        }
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };
    Ok(lowerer.finish(lowered))
}

struct EffectfulTargetLowerer {
    span: Span,
    next_temp: usize,
    stmts: Vec<Stmt>,
}

impl EffectfulTargetLowerer {
    fn new(span: Span) -> Self {
        Self {
            span,
            next_temp: 0,
            stmts: Vec::new(),
        }
    }

    fn stabilize(&mut self, expr: Expr) -> Expr {
        if can_replay_assignment_target(&expr) {
            return expr;
        }
        let name = format!(
            "__elephc_compound_{}_{}_{}",
            self.span.line, self.span.col, self.next_temp
        );
        self.next_temp += 1;
        self.stmts.push(Stmt::new(
            StmtKind::Assign {
                name: name.clone(),
                value: expr,
            },
            self.span,
        ));
        Expr::new(ExprKind::Variable(name), self.span)
    }

    fn finish(mut self, final_stmt: StmtKind) -> Stmt {
        self.stmts.push(Stmt::new(final_stmt, self.span));
        Stmt::new(StmtKind::Synthetic(self.stmts), self.span)
    }
}
