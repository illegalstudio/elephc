use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::parser::expr::{parse_assignment_value_expr, parse_expr};
use crate::span::Span;

use super::params::parse_type_expr;
use super::{expect_semicolon, expect_token};

#[derive(Debug, Clone, PartialEq)]
enum AssignmentOperator {
    Assign,
    Compound(crate::parser::ast::BinOp),
    NullCoalesce,
}

/// Handle statements starting with $variable: assignment, array ops, or post-increment.
pub(super) fn parse_variable_stmt(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let name = match &tokens[*pos].0 {
        Token::Variable(n) => n.clone(),
        _ => unreachable!(),
    };

    if let Some(stmt) = try_parse_postfix_assignment(tokens, pos, span)? {
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
        if let Some(op) = tokens.get(*pos).and_then(|(token, _)| assignment_operator(token)) {
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
                let value = assignment_value(target, op, rhs, span);
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
    parse_assign(tokens, pos, span)
}

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

    let op = assignment_operator(&tokens[*pos].0)
        .ok_or_else(|| CompileError::new(span, "Expected '=' after variable name"))?;
    *pos += 1;

    let rhs = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;

    let target = Expr::new(ExprKind::Variable(name.clone()), span);
    let value = assignment_value(target, op, rhs, span);

    Ok(Stmt::new(StmtKind::Assign { name, value }, span))
}

pub(super) fn try_parse_postfix_assignment(
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
    if op != AssignmentOperator::Assign && !can_replay_assignment_target(&lhs_expr) {
        return Err(CompileError::new(
            span,
            "Compound assignment target must be side-effect-free",
        ));
    }

    *pos = assign_pos + 1;
    let rhs = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
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

pub(super) fn try_parse_scoped_property_assignment(
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
    if op != AssignmentOperator::Assign && !can_replay_assignment_target(&lhs_expr) {
        return Err(CompileError::new(
            span,
            "Compound assignment target must be side-effect-free",
        ));
    }

    *pos = assign_pos + 1;
    let rhs = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
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

fn assignment_operator(token: &Token) -> Option<AssignmentOperator> {
    use crate::parser::ast::BinOp;

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

fn can_replay_assignment_target(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) | ExprKind::This | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, index } => {
            can_replay_assignment_target(array) && can_replay_assignment_target(index)
        }
        ExprKind::PropertyAccess { object, .. } => can_replay_assignment_target(object),
        ExprKind::BinaryOp { left, right, .. } => {
            can_replay_assignment_target(left) && can_replay_assignment_target(right)
        }
        ExprKind::InstanceOf { value, .. }
        | ExprKind::Negate(value)
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
        | ExprKind::ClassConstant { .. }
        | ExprKind::EnumCase { .. }
        | ExprKind::MagicConstant(_) => true,
        _ => false,
    }
}

/// Handle ++$var; or --$var; as standalone statements.
pub(super) fn parse_incdec_stmt(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let is_increment = tokens[*pos].0 == Token::PlusPlus;
    *pos += 1;

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

pub(super) fn parse_list_unpack(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume '['

    let mut vars = Vec::new();
    while *pos < tokens.len() && tokens[*pos].0 != Token::RBracket {
        if !vars.is_empty() {
            if tokens[*pos].0 != Token::Comma {
                return Err(CompileError::new(
                    tokens[*pos].1,
                    "Expected ',' between list variables",
                ));
            }
            *pos += 1;
        }
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Variable(n)) => {
                vars.push(n.clone());
                *pos += 1;
            }
            _ => {
                return Err(CompileError::new(
                    span,
                    "Expected variable in list unpacking",
                ))
            }
        }
    }

    if *pos >= tokens.len() || tokens[*pos].0 != Token::RBracket {
        return Err(CompileError::new(span, "Expected ']' after list variables"));
    }
    *pos += 1;

    expect_token(
        tokens,
        pos,
        &Token::Assign,
        "Expected '=' after list pattern",
    )?;

    let value = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;

    Ok(Stmt::new(StmtKind::ListUnpack { vars, value }, span))
}

pub(super) fn parse_global(
    tokens: &[(Token, Span)],
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

pub(super) fn parse_static_var(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'static'

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Variable(n)) => n.clone(),
        _ => return Err(CompileError::new(span, "Expected variable after 'static'")),
    };
    *pos += 1;

    expect_token(
        tokens,
        pos,
        &Token::Assign,
        "Expected '=' after static variable",
    )?;

    let init = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;

    Ok(Stmt::new(StmtKind::StaticVar { name, init }, span))
}

pub(super) fn looks_like_typed_assign(tokens: &[(Token, Span)], pos: usize) -> bool {
    let mut probe = pos;
    match parse_type_expr(tokens, &mut probe, tokens[pos].1) {
        Ok(_) => matches!(tokens.get(probe).map(|(t, _)| t), Some(Token::Variable(_))),
        Err(_) => false,
    }
}

pub(super) fn parse_typed_assign(
    tokens: &[(Token, Span)],
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
