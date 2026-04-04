use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::parser::expr::parse_expr;
use crate::span::Span;

use super::params::parse_type_expr;
use super::{expect_semicolon, expect_token};

/// Handle statements starting with $variable: assignment, array ops, or post-increment.
pub(super) fn parse_variable_stmt(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let start_pos = *pos;
    let name = match &tokens[*pos].0 {
        Token::Variable(n) => n.clone(),
        _ => unreachable!(),
    };

    // Property access/method call: $var->prop or $var->method()
    if *pos + 1 < tokens.len() && tokens[*pos + 1].0 == Token::Arrow {
        // Parse as expression first (handles $var->method() and chained access)
        let expr = parse_expr(tokens, pos)?;
        // Check if followed by assignment: $var->prop = value;
        if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
            *pos += 1;
            let value = parse_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            // Extract property from the expression
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

    // Array access: $var[...]
    if *pos + 1 < tokens.len() && tokens[*pos + 1].0 == Token::LBracket {
        *pos += 1; // consume $var
        *pos += 1; // consume [

        // $var[] = ... (push)
        if *pos < tokens.len() && tokens[*pos].0 == Token::RBracket {
            *pos += 1;
            expect_token(tokens, pos, &Token::Assign, "Expected '=' after '$var[]'")?;
            let value = parse_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            return Ok(Stmt::new(StmtKind::ArrayPush { array: name, value }, span));
        }

        // $var[index] = ...
        let index = parse_expr(tokens, pos)?;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::RBracket {
            return Err(CompileError::new(span, "Expected ']'"));
        }
        *pos += 1;

        if *pos < tokens.len() && tokens[*pos].0 == Token::Arrow {
            *pos = start_pos;
            let expr = parse_expr(tokens, pos)?;
            if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                *pos += 1;
                let value = parse_expr(tokens, pos)?;
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

        if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
            *pos += 1;
            let value = parse_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            return Ok(Stmt::new(
                StmtKind::ArrayAssign {
                    array: name,
                    index,
                    value,
                },
                span,
            ));
        }

        return Err(CompileError::new(span, "Expected '=' after array access"));
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

    use crate::parser::ast::BinOp;
    let compound_op = match &tokens[*pos].0 {
        Token::PlusAssign => Some(BinOp::Add),
        Token::MinusAssign => Some(BinOp::Sub),
        Token::StarAssign => Some(BinOp::Mul),
        Token::SlashAssign => Some(BinOp::Div),
        Token::PercentAssign => Some(BinOp::Mod),
        Token::DotAssign => Some(BinOp::Concat),
        Token::Assign => None,
        _ => return Err(CompileError::new(span, "Expected '=' after variable name")),
    };
    *pos += 1;

    let rhs = parse_expr(tokens, pos)?;
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

    let value = parse_expr(tokens, pos)?;
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

    let init = parse_expr(tokens, pos)?;
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
    let value = parse_expr(tokens, pos)?;
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
